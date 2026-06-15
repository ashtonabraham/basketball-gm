//! The top-level league: all state plus the season state machine.
//!
//! This is the single object the UI holds. It is fully `serde`-serializable, so
//! the web layer can save/load it to the browser's localStorage as JSON.

use crate::draft::{Draft, DraftPick, ScoutEntry};
use crate::names::{FIRST_NAMES, LAST_NAMES};
use crate::player::{Player, Ratings, SeasonStats};
use crate::playoffs::{first_round_pairs, high_seed_hosts, Playoffs, Series};
use crate::schedule::{generate_schedule, Game, GameResult};
use crate::sim::{simulate_game, TeamBox};
use crate::standings::{conference_standings, playoff_seeds};
use crate::team::Team;
use crate::teams_data::PRESETS;
use crate::types::{Color, Conference, PlayerId, Position, TeamId};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Generate a player's 8 attributes from a team talent baseline and the
/// position's identity (e.g. centers dunk and rebound; guards handle and pass).
fn gen_ratings(pos: Position, talent: f64, rng: &mut impl Rng) -> Ratings {
    let base = talent + rng.gen_range(-12.0..14.0);
    let attr = |modifier: f64, rng: &mut dyn rand::RngCore| -> u8 {
        (base + modifier + rng.gen_range(-8.0..8.0)).clamp(25.0, 99.0) as u8
    };
    // (layup, dunk, three, passing, ball_handling, rebounding, defense, athleticism)
    let m = match pos {
        Position::PG => (-2.0, -10.0, 6.0, 12.0, 12.0, -10.0, 0.0, 6.0),
        Position::SG => (0.0, -4.0, 10.0, 2.0, 6.0, -6.0, 0.0, 5.0),
        Position::SF => (3.0, 2.0, 2.0, 0.0, 0.0, 0.0, 2.0, 3.0),
        Position::PF => (5.0, 8.0, -4.0, -4.0, -6.0, 8.0, 3.0, 0.0),
        Position::C => (6.0, 12.0, -10.0, -6.0, -10.0, 12.0, 5.0, -2.0),
    };
    Ratings {
        layup: attr(m.0, rng),
        dunk: attr(m.1, rng),
        three: attr(m.2, rng),
        passing: attr(m.3, rng),
        ball_handling: attr(m.4, rng),
        rebounding: attr(m.5, rng),
        defense: attr(m.6, rng),
        athleticism: attr(m.7, rng),
    }
}

/// A player's peak-overall ceiling: current overall plus age-dependent upside.
fn gen_potential(ovr: u8, age: u8, rng: &mut impl Rng) -> u8 {
    let room: u32 = match age {
        0..=20 => rng.gen_range(6..=26),
        21..=22 => rng.gen_range(4..=20),
        23..=24 => rng.gen_range(2..=12),
        25..=26 => rng.gen_range(0..=6),
        _ => 0,
    };
    (ovr as u32 + room).min(99) as u8
}

/// Apply an approximately uniform per-attribute change of `d` (with a little
/// noise), clamped to a sane range. Raising every attribute by `d` moves the
/// overall by roughly `d`.
fn apply_attr_delta(r: &mut Ratings, d: i32, rng: &mut impl Rng) {
    let bump = |v: &mut u8, rng: &mut dyn rand::RngCore| {
        *v = (*v as i32 + d + rng.gen_range(-1..=1)).clamp(25, 99) as u8;
    };
    bump(&mut r.layup, rng);
    bump(&mut r.dunk, rng);
    bump(&mut r.three, rng);
    bump(&mut r.passing, rng);
    bump(&mut r.ball_handling, rng);
    bump(&mut r.rebounding, rng);
    bump(&mut r.defense, rng);
    bump(&mut r.athleticism, rng);
}

/// One offseason of development: young players grow toward their potential,
/// veterans decline. Called after ages are incremented.
fn develop_player(p: &mut Player, rng: &mut impl Rng) {
    let ovr = p.overall();
    if p.age <= 26 && ovr < p.potential {
        let room = (p.potential - ovr) as f64;
        let gain = (room * rng.gen_range(0.20..0.50)).round() as i32;
        let gain = gain.max(1).min((p.potential - ovr) as i32);
        apply_attr_delta(&mut p.ratings, gain, rng);
    } else if p.age >= 31 {
        let severity = (p.age as i32 - 30).max(0);
        let loss = (rng.gen_range(1.0..3.0) + severity as f64 * 0.4).round() as i32;
        apply_attr_delta(&mut p.ratings, -loss, rng);
    }
}

/// End-of-season individual awards (winners by player id).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Awards {
    pub mvp: Option<PlayerId>,
    pub dpoy: Option<PlayerId>,
    pub roy: Option<PlayerId>,
}

/// How the owner felt about the season.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OwnerTone {
    TooEarly,
    Pleased,
    Neutral,
    Displeased,
}

/// The owner's after-season note to the GM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnerMessage {
    pub tone: OwnerTone,
    pub body: String,
}

/// Add one game's box score into the running season totals (indexed by id).
fn accumulate_stats(stats: &mut [SeasonStats], tb: &TeamBox) {
    for l in &tb.lines {
        let s = &mut stats[l.player_id as usize];
        s.gp += 1;
        s.min += l.min;
        s.pts += l.pts;
        s.fgm += l.fgm;
        s.fga += l.fga;
        s.tpm += l.tpm;
        s.tpa += l.tpa;
        s.ftm += l.ftm;
        s.fta += l.fta;
        s.oreb += l.oreb;
        s.dreb += l.dreb;
        s.ast += l.ast;
        s.stl += l.stl;
        s.blk += l.blk;
        s.tov += l.tov;
    }
}

/// Where we are in the yearly cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    /// Picking and customizing your team.
    TeamSelect,
    RegularSeason,
    Playoffs,
    /// Season over; recap is available.
    Offseason,
    /// The annual draft is running.
    Draft,
}

/// How a team's season ended, for the recap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayoffOutcome {
    MissedPlayoffs,
    LostInRound(usize), // 0=first round .. 3=finals
    WonChampionship,
}

/// End-of-season summary focused on the user's team.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeasonRecap {
    pub season: u32,
    pub team_name: String,
    pub wins: u32,
    pub losses: u32,
    pub conference_seed: Option<u32>,
    pub outcome: PlayoffOutcome,
    pub champion_name: String,
    pub best_player: String,
    pub best_player_ovr: u8,
}

/// Compact record of a finished season, kept in league history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeasonHistory {
    pub season: u32,
    pub champion_id: TeamId,
    pub champion_name: String,
    pub user_wins: u32,
    pub user_losses: u32,
    pub user_outcome: PlayoffOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct League {
    pub season: u32,
    pub phase: Phase,
    pub teams: Vec<Team>,
    pub players: Vec<Player>,
    pub schedule: Vec<Game>,
    pub user_team_id: Option<TeamId>,
    pub playoffs: Option<Playoffs>,
    pub history: Vec<SeasonHistory>,
    /// Accumulated regular-season stats, indexed by player id.
    pub season_stats: Vec<SeasonStats>,
    /// Accumulated Finals-only stats (for Finals MVP), indexed by player id.
    pub finals_stats: Vec<SeasonStats>,
    /// The draft, while one is running.
    pub draft: Option<Draft>,
    /// Awards from the most recently completed season.
    pub awards: Option<Awards>,
    /// The owner's message after the most recently completed season.
    pub owner_message: Option<OwnerMessage>,
    seed: u64,
    next_player_id: PlayerId,
}

impl League {
    /// Build a fresh league from the 32 presets with generated rosters and a
    /// full schedule. Starts in `TeamSelect`.
    pub fn new(seed: u64) -> Self {
        let mut league = League {
            season: 1,
            phase: Phase::TeamSelect,
            teams: Vec::with_capacity(32),
            players: Vec::new(),
            schedule: Vec::new(),
            user_team_id: None,
            playoffs: None,
            history: Vec::new(),
            season_stats: Vec::new(),
            finals_stats: Vec::new(),
            draft: None,
            awards: None,
            owner_message: None,
            seed,
            next_player_id: 0,
        };

        // Create teams from presets.
        for (i, p) in PRESETS.iter().enumerate() {
            league.teams.push(Team {
                id: i as TeamId,
                location: p.location.to_string(),
                name: p.name.to_string(),
                abbrev: p.abbrev.to_string(),
                primary: p.primary_color(),
                secondary: p.secondary_color(),
                conference: p.conference,
                roster: Vec::new(),
                wins: 0,
                losses: 0,
            });
        }

        // Generate rosters. Each team gets a random talent level so the league
        // has contenders and bottom-feeders.
        let mut rng = StdRng::seed_from_u64(seed ^ 0xA11CE);
        let team_ids: Vec<TeamId> = league.teams.iter().map(|t| t.id).collect();
        for tid in &team_ids {
            let talent = rng.gen_range(38.0..64.0);
            league.generate_roster(*tid, talent, &mut rng);
        }

        // Size the season-stats table to match the generated players.
        league.season_stats = vec![SeasonStats::default(); league.players.len()];

        // Build the schedule.
        let mut sched_rng = StdRng::seed_from_u64(seed ^ 0x5C4ED_u64);
        league.schedule = generate_schedule(&team_ids, &mut sched_rng);

        league
    }

    // ---- Team builder ----

    /// Choose which team the user controls and move into the regular season.
    pub fn select_team(&mut self, team_id: TeamId) {
        self.user_team_id = Some(team_id);
        self.phase = Phase::RegularSeason;
    }

    /// Edit the user-facing fields of a team (used by the team builder UI).
    pub fn customize_team(
        &mut self,
        team_id: TeamId,
        name: Option<String>,
        abbrev: Option<String>,
        primary: Option<Color>,
        secondary: Option<Color>,
    ) {
        if let Some(t) = self.teams.iter_mut().find(|t| t.id == team_id) {
            if let Some(n) = name {
                t.name = n;
            }
            if let Some(a) = abbrev {
                t.abbrev = a;
            }
            if let Some(c) = primary {
                t.primary = c;
            }
            if let Some(c) = secondary {
                t.secondary = c;
            }
        }
    }

    // ---- Roster / player generation ----

    fn generate_roster(&mut self, team_id: TeamId, talent: f64, rng: &mut impl Rng) {
        // A standard distribution of positions for a 14-man roster.
        const SLOTS: [Position; 14] = [
            Position::PG, Position::PG, Position::PG,
            Position::SG, Position::SG, Position::SG,
            Position::SF, Position::SF, Position::SF,
            Position::PF, Position::PF, Position::PF,
            Position::C, Position::C,
        ];
        for pos in SLOTS {
            let player = self.make_player(team_id, pos, talent, rng);
            let pid = player.id;
            self.players.push(player);
            if let Some(t) = self.teams.iter_mut().find(|t| t.id == team_id) {
                t.roster.push(pid);
            }
        }
    }

    fn make_player(
        &mut self,
        team_id: TeamId,
        pos: Position,
        talent: f64,
        rng: &mut impl Rng,
    ) -> Player {
        let id = self.next_player_id;
        self.next_player_id += 1;

        let first = FIRST_NAMES[rng.gen_range(0..FIRST_NAMES.len())];
        let last = LAST_NAMES[rng.gen_range(0..LAST_NAMES.len())];
        let name = format!("{first} {last}");

        let age = rng.gen_range(19..=38);
        let ratings = gen_ratings(pos, talent, rng);
        let potential = gen_potential(ratings.overall(), age, rng);
        Player { id, name, age, position: pos, ratings, potential, team: Some(team_id), draft_season: None }
    }

    // ---- Regular season ----

    /// The next day with unplayed games, or `None` if the season is over.
    pub fn current_day(&self) -> Option<u32> {
        self.schedule
            .iter()
            .filter(|g| !g.is_played())
            .map(|g| g.day)
            .min()
    }

    pub fn regular_season_complete(&self) -> bool {
        self.current_day().is_none()
    }

    /// Simulate every game on the given day, updating records and stats.
    fn sim_specific_day(&mut self, day: u32) {
        let mut rng = StdRng::seed_from_u64(self.seed.wrapping_mul(1_000_003).wrapping_add(day as u64));
        let indices: Vec<usize> = self
            .schedule
            .iter()
            .enumerate()
            .filter(|(_, g)| g.day == day && !g.is_played())
            .map(|(i, _)| i)
            .collect();

        // Simulate first (immutable borrows of teams/players only).
        let mut sims = Vec::with_capacity(indices.len());
        for &i in &indices {
            let home_id = self.schedule[i].home;
            let away_id = self.schedule[i].away;
            let home = self.teams.iter().find(|t| t.id == home_id).unwrap();
            let away = self.teams.iter().find(|t| t.id == away_id).unwrap();
            let g = simulate_game(home, away, &self.players, &mut rng);
            sims.push((i, g));
        }

        // Apply results: stats, records, and the stored final score.
        for (i, g) in sims {
            accumulate_stats(&mut self.season_stats, &g.home);
            accumulate_stats(&mut self.season_stats, &g.away);
            let res = GameResult { home_score: g.home.score, away_score: g.away.score };
            let home_won = res.home_won();
            if let Some(t) = self.teams.iter_mut().find(|t| t.id == g.home.team_id) {
                if home_won { t.wins += 1 } else { t.losses += 1 }
            }
            if let Some(t) = self.teams.iter_mut().find(|t| t.id == g.away.team_id) {
                if home_won { t.losses += 1 } else { t.wins += 1 }
            }
            self.schedule[i].result = Some(res);
        }
    }

    /// Simulate the next day of games. Returns the day simmed, or `None` if the
    /// regular season is already complete.
    pub fn sim_day(&mut self) -> Option<u32> {
        let day = self.current_day()?;
        self.sim_specific_day(day);
        if self.regular_season_complete() {
            self.phase = Phase::RegularSeason; // stays until playoffs are started
        }
        Some(day)
    }

    /// Sim up to `days` days (e.g. a week). Stops at the end of the season.
    pub fn sim_days(&mut self, days: u32) {
        for _ in 0..days {
            if self.sim_day().is_none() {
                break;
            }
        }
    }

    /// Sim the rest of the regular season.
    pub fn sim_to_end_of_season(&mut self) {
        while self.sim_day().is_some() {}
    }

    // ---- Playoffs (advanced one game-day at a time) ----

    /// Map every team to its conference seed (1 = best) for home-court.
    fn seed_map(&self) -> HashMap<TeamId, u32> {
        let mut m = HashMap::new();
        for conf in [Conference::East, Conference::West] {
            for row in conference_standings(&self.teams, conf) {
                m.insert(row.team_id, row.seed);
            }
        }
        m
    }

    /// Seed the first-round bracket from final standings. No games are played
    /// yet — use the `playoff_sim_*` methods to advance.
    pub fn start_playoffs(&mut self) {
        assert!(self.regular_season_complete(), "regular season not finished");
        let east = playoff_seeds(&self.teams, Conference::East);
        let west = playoff_seeds(&self.teams, Conference::West);

        let mut round0 = Vec::with_capacity(8);
        for (h, l) in first_round_pairs(&east).into_iter().chain(first_round_pairs(&west)) {
            round0.push(Series::new(h, l));
        }
        self.playoffs = Some(Playoffs { rounds: vec![round0], champion: None, finals_mvp: None });
        self.finals_stats = vec![SeasonStats::default(); self.players.len()];
        self.phase = Phase::Playoffs;
    }

    /// Is the current (deepest) round the Finals?
    fn in_finals(&self) -> bool {
        self.playoffs.as_ref().map(|p| p.rounds.len() == 4).unwrap_or(false)
    }

    /// Among the champion's roster, the best Finals performer.
    fn compute_finals_mvp(&self, champion: TeamId) -> Option<PlayerId> {
        self.teams
            .iter()
            .find(|t| t.id == champion)?
            .roster
            .iter()
            .filter_map(|pid| {
                let s = self.finals_stats.get(*pid as usize)?;
                if s.gp == 0 {
                    return None;
                }
                let score = s.pts as f64 + s.reb() as f64 * 0.7 + s.ast as f64 * 0.7;
                Some((*pid, score))
            })
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .map(|(pid, _)| pid)
    }

    pub fn playoffs_complete(&self) -> bool {
        self.playoffs.as_ref().map(|p| p.champion.is_some()).unwrap_or(false)
    }

    /// Is the current (deepest) round fully decided?
    fn current_round_decided(&self) -> bool {
        self.playoffs
            .as_ref()
            .and_then(|p| p.rounds.last())
            .map(|r| r.iter().all(|s| s.is_decided()))
            .unwrap_or(true)
    }

    /// Does the user's team have a game on the next game-day (i.e. it is in an
    /// undecided series in the current round)?
    pub fn user_plays_next_playoff_game(&self) -> bool {
        let Some(uid) = self.user_team_id else { return false };
        let Some(po) = &self.playoffs else { return false };
        if po.champion.is_some() {
            return false;
        }
        po.rounds
            .last()
            .map(|r| r.iter().any(|s| !s.is_decided() && s.has_team(uid)))
            .unwrap_or(false)
    }

    /// Crown the Finals winner and pick the Finals MVP.
    fn crown_champion(&mut self) {
        let champ = self.playoffs.as_ref().and_then(|p| p.rounds[3][0].winner());
        let mvp = champ.and_then(|c| self.compute_finals_mvp(c));
        if let Some(po) = self.playoffs.as_mut() {
            po.champion = champ;
            po.finals_mvp = mvp;
        }
    }

    /// Build the next round's pairings from the current round's winners.
    fn build_next_round(&mut self) {
        let seed_of = self.seed_map();
        let Some(po) = self.playoffs.as_mut() else { return };
        let last = po.rounds.last().unwrap();
        let winners: Vec<TeamId> = last.iter().map(|s| s.winner().unwrap()).collect();
        // Pair adjacent winners; the better seed becomes the higher seed.
        let mut next = Vec::with_capacity(winners.len() / 2);
        for pair in winners.chunks(2) {
            let (a, b) = (pair[0], pair[1]);
            let a_seed = *seed_of.get(&a).unwrap_or(&99);
            let b_seed = *seed_of.get(&b).unwrap_or(&99);
            let (high, low) = if a_seed <= b_seed { (a, b) } else { (b, a) };
            next.push(Series::new(high, low));
        }
        po.rounds.push(next);
    }

    /// Play exactly one game in the given series of the current round.
    fn play_playoff_game(&mut self, series_idx: usize, rng: &mut impl Rng) {
        // Read the matchup and home/away first (immutable borrows).
        let (home_id, away_id) = {
            let po = self.playoffs.as_ref().unwrap();
            let s = &po.rounds.last().unwrap()[series_idx];
            let game_no = s.games_played() + 1;
            if high_seed_hosts(game_no) {
                (s.high, s.low)
            } else {
                (s.low, s.high)
            }
        };
        let is_finals = self.in_finals();
        let home = self.teams.iter().find(|t| t.id == home_id).unwrap();
        let away = self.teams.iter().find(|t| t.id == away_id).unwrap();
        let g = simulate_game(home, away, &self.players, rng);
        let res = GameResult { home_score: g.home.score, away_score: g.away.score };

        // Finals games feed the Finals MVP race.
        if is_finals {
            accumulate_stats(&mut self.finals_stats, &g.home);
            accumulate_stats(&mut self.finals_stats, &g.away);
        }

        // Record the result.
        let po = self.playoffs.as_mut().unwrap();
        let round = po.rounds.last_mut().unwrap();
        let s = &mut round[series_idx];
        let high_won = if home_id == s.high { res.home_won() } else { !res.home_won() };
        if high_won {
            s.high_wins += 1;
        } else {
            s.low_wins += 1;
        }
        s.games.push(res);
    }

    /// Advance one play-off game-day: every live series in the current round
    /// plays its next game. Auto-builds the next round (or crowns a champion)
    /// when a round completes. Returns false if the playoffs are over.
    pub fn playoff_sim_gameday(&mut self) -> bool {
        if self.playoffs.is_none() || self.playoffs_complete() {
            return false;
        }
        // If the current round is done, advance the bracket before playing.
        if self.current_round_decided() {
            let last_idx = self.playoffs.as_ref().unwrap().rounds.len() - 1;
            if last_idx >= 3 {
                // Finals decided — crown the champion.
                self.crown_champion();
                return false;
            }
            self.build_next_round();
        }

        // Play one game in each undecided series of the current round.
        let n = self.playoffs.as_ref().unwrap().rounds.last().unwrap().len();
        let played = self.playoffs.as_ref().unwrap().rounds.iter().flatten().map(|s| s.games.len()).sum::<usize>();
        let mut rng = StdRng::seed_from_u64(self.seed ^ 0x71A04FF_u64 ^ played as u64);
        for i in 0..n {
            let undecided = !self.playoffs.as_ref().unwrap().rounds.last().unwrap()[i].is_decided();
            if undecided {
                self.play_playoff_game(i, &mut rng);
            }
        }

        // If that completed the Finals, crown immediately.
        if self.current_round_decided() {
            let last_idx = self.playoffs.as_ref().unwrap().rounds.len() - 1;
            if last_idx >= 3 {
                self.crown_champion();
            }
        }
        true
    }

    /// Sim game-days until the user's team is about to play (so the user can sim
    /// that game themselves), or the playoffs end.
    pub fn playoff_sim_to_user_game(&mut self) {
        // If the user already has a game pending now, do nothing.
        while !self.playoffs_complete() && !self.user_plays_next_playoff_game() {
            if !self.playoff_sim_gameday() {
                break;
            }
        }
    }

    /// Is the user's team still alive in the bracket (not yet eliminated)?
    pub fn user_still_in_playoffs(&self) -> bool {
        let Some(uid) = self.user_team_id else { return false };
        let Some(po) = &self.playoffs else { return false };
        if po.champion == Some(uid) {
            return true;
        }
        // Alive if it appears in the current (deepest) round.
        po.rounds.last().map(|r| r.iter().any(|s| s.has_team(uid))).unwrap_or(false)
    }

    /// Sim game-days until the current round (the deepest at call time) is fully
    /// decided, then stop before the next round begins.
    pub fn playoff_sim_round(&mut self) {
        let Some(target) = self.playoffs.as_ref().map(|p| p.rounds.len() - 1) else { return };
        while !self.playoffs_complete() {
            let cur = self.playoffs.as_ref().unwrap().rounds.len() - 1;
            if cur == target && self.current_round_decided() {
                break;
            }
            if !self.playoff_sim_gameday() {
                break;
            }
        }
    }

    /// Sim the entire remaining postseason.
    pub fn playoff_sim_all(&mut self) {
        while self.playoff_sim_gameday() {}
    }

    // ---- Recap / new season ----

    /// Find where a team finished in the playoffs.
    fn outcome_for(&self, team_id: TeamId) -> PlayoffOutcome {
        let Some(po) = &self.playoffs else {
            return PlayoffOutcome::MissedPlayoffs;
        };
        if po.champion == Some(team_id) {
            return PlayoffOutcome::WonChampionship;
        }
        // Find the deepest round the team appeared in and lost.
        let mut deepest: Option<usize> = None;
        for (ri, round) in po.rounds.iter().enumerate() {
            for s in round {
                if s.high == team_id || s.low == team_id {
                    deepest = Some(ri);
                }
            }
        }
        match deepest {
            Some(r) => PlayoffOutcome::LostInRound(r),
            None => PlayoffOutcome::MissedPlayoffs,
        }
    }

    fn team_name(&self, id: TeamId) -> String {
        self.teams
            .iter()
            .find(|t| t.id == id)
            .map(|t| t.full_name())
            .unwrap_or_default()
    }

    /// Build the end-of-season recap for the user's team. Call after playoffs.
    pub fn season_recap(&self) -> Option<SeasonRecap> {
        let uid = self.user_team_id?;
        let team = self.teams.iter().find(|t| t.id == uid)?;
        let champion_id = self.playoffs.as_ref().and_then(|p| p.champion);

        // Conference seed (1..16), if any.
        let seeds = crate::standings::conference_standings(&self.teams, team.conference);
        let conference_seed = seeds
            .iter()
            .find(|r| r.team_id == uid)
            .map(|r| r.seed)
            .filter(|s| *s <= 8); // only meaningful if a playoff team

        // Best player on the roster.
        let (best_player, best_player_ovr) = team
            .roster
            .iter()
            .filter_map(|pid| self.players.iter().find(|p| p.id == *pid))
            .max_by_key(|p| p.overall())
            .map(|p| (p.name.clone(), p.overall()))
            .unwrap_or_default();

        Some(SeasonRecap {
            season: self.season,
            team_name: team.full_name(),
            wins: team.wins,
            losses: team.losses,
            conference_seed,
            outcome: self.outcome_for(uid),
            champion_name: champion_id.map(|c| self.team_name(c)).unwrap_or_default(),
            best_player,
            best_player_ovr,
        })
    }

    // ---- Awards & the owner ----

    /// A simple production value (per game) for awards.
    fn award_value(s: &SeasonStats) -> f64 {
        let stl = s.stl as f64 / s.gp.max(1) as f64;
        let blk = s.blk as f64 / s.gp.max(1) as f64;
        s.ppg() + s.rpg() * 1.2 + s.apg() * 1.5 + (stl + blk) * 2.0
    }

    /// Compute MVP, Defensive Player of the Year, and Rookie of the Year.
    pub fn compute_awards(&self) -> Awards {
        let team_winpct = |pid: PlayerId| -> f64 {
            self.players
                .iter()
                .find(|p| p.id == pid)
                .and_then(|p| p.team)
                .and_then(|tid| self.teams.iter().find(|t| t.id == tid))
                .map(|t| t.win_pct())
                .unwrap_or(0.0)
        };

        // MVP: production weighted by team success (min 30 games).
        let mvp = self
            .players
            .iter()
            .filter(|p| self.season_stats[p.id as usize].gp >= 30)
            .map(|p| (p.id, Self::award_value(&self.season_stats[p.id as usize]) + team_winpct(p.id) * 12.0))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .map(|(id, _)| id);

        // DPOY: defense-heavy score.
        let dpoy = self
            .players
            .iter()
            .filter(|p| self.season_stats[p.id as usize].gp >= 30)
            .map(|p| {
                let s = &self.season_stats[p.id as usize];
                let stl = s.stl as f64 / s.gp.max(1) as f64;
                let blk = s.blk as f64 / s.gp.max(1) as f64;
                let score = stl * 3.0 + blk * 3.0 + s.rpg() * 0.7 + p.ratings.defense as f64 * 0.15;
                (p.id, score)
            })
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .map(|(id, _)| id);

        // ROY: best rookie (drafted last offseason), min 20 games.
        let rookie_class = self.season.checked_sub(1);
        let roy = self
            .players
            .iter()
            .filter(|p| p.draft_season == rookie_class && self.season_stats[p.id as usize].gp >= 20)
            .map(|p| (p.id, Self::award_value(&self.season_stats[p.id as usize])))
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .map(|(id, _)| id);

        Awards { mvp, dpoy, roy }
    }

    /// League-average points per team game.
    fn league_ppg(&self) -> f64 {
        let played: Vec<&crate::schedule::Game> = self.schedule.iter().filter(|g| g.is_played()).collect();
        if played.is_empty() {
            return 110.0;
        }
        let total: u32 = played.iter().filter_map(|g| g.result).map(|r| r.home_score + r.away_score).sum();
        total as f64 / (played.len() as f64 * 2.0)
    }

    /// A team's points scored / allowed per game.
    fn team_points(&self, tid: TeamId) -> (f64, f64) {
        let mut pf = 0u32;
        let mut pa = 0u32;
        let mut n = 0u32;
        for g in self.schedule.iter().filter(|g| g.is_played()) {
            let Some(r) = g.result else { continue };
            if g.home == tid {
                pf += r.home_score;
                pa += r.away_score;
                n += 1;
            } else if g.away == tid {
                pf += r.away_score;
                pa += r.home_score;
                n += 1;
            }
        }
        if n == 0 {
            (0.0, 0.0)
        } else {
            (pf as f64 / n as f64, pa as f64 / n as f64)
        }
    }

    fn team_three_pct(&self, tid: TeamId) -> f64 {
        let Some(team) = self.teams.iter().find(|t| t.id == tid) else { return 0.0 };
        let (mut m, mut a) = (0u32, 0u32);
        for pid in &team.roster {
            let s = &self.season_stats[*pid as usize];
            m += s.tpm;
            a += s.tpa;
        }
        if a == 0 { 0.0 } else { m as f64 / a as f64 }
    }

    /// The single thing the owner wants improved, or `None` if nothing glaring.
    fn owner_goal(&self, uid: TeamId) -> Option<String> {
        let outcome = self.outcome_for(uid);
        if matches!(outcome, PlayoffOutcome::MissedPlayoffs) {
            return Some("get this team into the playoffs".into());
        }
        let (pf, pa) = self.team_points(uid);
        let lppg = self.league_ppg();
        if pa - lppg > 3.0 {
            return Some("tighten up the defense — we give up too many points".into());
        }
        if lppg - pf > 3.0 {
            return Some("find a way to put more points on the board".into());
        }
        if self.team_three_pct(uid) < 0.34 {
            return Some("get more shooting into this lineup".into());
        }
        match outcome {
            PlayoffOutcome::LostInRound(0) => Some("get out of the first round next year".into()),
            PlayoffOutcome::LostInRound(1) => Some("reach the conference finals".into()),
            PlayoffOutcome::LostInRound(2) => Some("break through to the Finals".into()),
            PlayoffOutcome::LostInRound(3) => Some("we were right there — finish the job and win it all".into()),
            _ => None,
        }
    }

    /// Build the owner's after-season message for the user's team.
    pub fn evaluate_owner(&self) -> OwnerMessage {
        let Some(uid) = self.user_team_id else {
            return OwnerMessage { tone: OwnerTone::Neutral, body: String::new() };
        };
        let team = self.teams.iter().find(|t| t.id == uid).unwrap();
        let city = team.location.clone();
        let champ = self.playoffs.as_ref().and_then(|p| p.champion);

        // Hands-off while you build (first three seasons).
        if self.season <= 3 {
            return OwnerMessage {
                tone: OwnerTone::TooEarly,
                body: format!(
                    "It's only year {}. I'm not going to judge you yet — take your time and build me something special.",
                    self.season
                ),
            };
        }

        // A title earns the warmest words.
        if champ == Some(uid) {
            return OwnerMessage {
                tone: OwnerTone::Pleased,
                body: format!("You brought a championship home to {city}! I knew I hired the right person. Keep it up, my son."),
            };
        }

        // Roster-based expectation vs reality.
        let league_avg: f64 = self.teams.iter().map(|t| t.strength(&self.players)).sum::<f64>() / self.teams.len() as f64;
        let my_strength = team.strength(&self.players);
        let exp_pct = (0.5 + (my_strength - league_avg) * 0.02).clamp(0.18, 0.82);
        let expected_wins = (exp_pct * 82.0).round() as i32;
        let win_diff = team.wins as i32 - expected_wins;
        let made_playoffs = !matches!(self.outcome_for(uid), PlayoffOutcome::MissedPlayoffs);
        let deep_run = matches!(self.outcome_for(uid), PlayoffOutcome::LostInRound(2) | PlayoffOutcome::LostInRound(3));

        let goal = self.owner_goal(uid);

        let tone = if !made_playoffs || win_diff <= -7 {
            OwnerTone::Displeased
        } else if win_diff >= 5 || deep_run {
            OwnerTone::Pleased
        } else {
            OwnerTone::Neutral
        };

        let record = format!("{}-{}", team.wins, team.losses);
        let body = match (tone, goal) {
            (OwnerTone::Pleased, None) => {
                format!("Hell of a year — {record} and you had us believing. Keep it up, my son.")
            }
            (OwnerTone::Pleased, Some(g)) => {
                format!("Strong season at {record}. I liked what I saw. One thing for next year: {g}.")
            }
            (OwnerTone::Neutral, Some(g)) => {
                format!("A {record} season — respectable, nothing to hang our heads about. But next year, I want you to {g}.")
            }
            (OwnerTone::Neutral, None) => {
                format!("A solid {record} campaign. Keep it up, my son.")
            }
            (OwnerTone::Displeased, Some(g)) => {
                format!("I'll be straight with you — I expected more than {record}. Next season, {g}, or you and I are going to have a problem.")
            }
            (OwnerTone::Displeased, None) => {
                format!("{record} is not what I'm paying for. I expect better next year.")
            }
            (OwnerTone::TooEarly, _) => unreachable!(),
        };

        OwnerMessage { tone, body }
    }

    /// Record the finished season into history and move to the offseason.
    pub fn finish_season(&mut self) {
        // Awards and the owner's note use this season's data; evaluate before
        // pushing history so the owner can compare to last year.
        self.awards = Some(self.compute_awards());
        self.owner_message = Some(self.evaluate_owner());

        let champion_id = self.playoffs.as_ref().and_then(|p| p.champion);
        if let (Some(uid), Some(cid)) = (self.user_team_id, champion_id) {
            let user = self.teams.iter().find(|t| t.id == uid);
            if let Some(u) = user {
                self.history.push(SeasonHistory {
                    season: self.season,
                    champion_id: cid,
                    champion_name: self.team_name(cid),
                    user_wins: u.wins,
                    user_losses: u.losses,
                    user_outcome: self.outcome_for(uid),
                });
            }
        }
        self.phase = Phase::Offseason;
    }

    // ---- Draft ----

    /// Generate a prospect class and the lottery-seeded 2-round draft order,
    /// then move into the draft phase. Call after `finish_season`.
    pub fn enter_draft(&mut self) {
        let mut rng = StdRng::seed_from_u64(self.seed ^ 0xD4AF7_u64 ^ (self.season as u64));

        // --- Generate ~70 prospects (young, with a draft-class talent curve). ---
        let mut prospects: Vec<PlayerId> = Vec::new();
        let class_size = 70;
        for i in 0..class_size {
            // Earlier prospects skew more talented, with randomness so the board
            // isn't perfectly ordered.
            let curve = 60.0 - (i as f64 / class_size as f64) * 22.0;
            let mut talent = curve + rng.gen_range(-9.0..9.0);
            if rng.gen::<f64>() < 0.08 {
                talent += 8.0; // occasional stud
            }
            let pos = Position::ALL[rng.gen_range(0..5)];
            let id = self.next_player_id;
            self.next_player_id += 1;
            let first = FIRST_NAMES[rng.gen_range(0..FIRST_NAMES.len())];
            let last = LAST_NAMES[rng.gen_range(0..LAST_NAMES.len())];
            let age = rng.gen_range(19..=22);
            let ratings = gen_ratings(pos, talent, &mut rng);
            // Prospects skew toward upside: a guaranteed chunk of growth room.
            let base_pot = gen_potential(ratings.overall(), age, &mut rng);
            let potential = (base_pot as u32 + rng.gen_range(0..=6)).min(99) as u8;
            self.players.push(Player {
                id,
                name: format!("{first} {last}"),
                age,
                position: pos,
                ratings,
                potential,
                team: None,
                draft_season: None,
            });
            prospects.push(id);
        }
        // Keep the season-stats table sized to the player list.
        self.season_stats.resize(self.players.len(), SeasonStats::default());

        // --- Draft order ---
        let order = self.draft_order(&mut rng);
        let mut picks = Vec::with_capacity(64);
        for round in 1..=2u8 {
            for (i, &team_id) in order.iter().enumerate() {
                picks.push(DraftPick {
                    round,
                    overall: ((round as usize - 1) * 32 + i + 1) as u8,
                    team_id,
                    player_id: None,
                });
            }
        }

        // Initial (fuzzy) scouting read on every prospect.
        let mut scouting = HashMap::new();
        for &pid in &prospects {
            let pot = self.players.iter().find(|p| p.id == pid).map(|p| p.potential as f64).unwrap_or(50.0);
            let uncertainty = 14.0;
            let noise = (rng.gen::<f64>() + rng.gen::<f64>() - 1.0) * uncertainty; // ~[-u, u], centered
            scouting.insert(pid, ScoutEntry { estimate: (pot + noise).clamp(30.0, 99.0), uncertainty });
        }

        self.draft = Some(Draft { picks, prospects, on_clock: 0, scouting, scout_points: 25 });
        self.phase = Phase::Draft;
    }

    /// Spend one scouting point on a prospect: tighten the uncertainty and
    /// re-estimate his potential (the grade may rise or fall as info improves).
    pub fn scout_prospect(&mut self, pid: PlayerId) {
        let remaining = self.draft.as_ref().map(|d| d.scout_points).unwrap_or(0);
        if remaining == 0 {
            return;
        }
        let pot = match self.players.iter().find(|p| p.id == pid) {
            Some(p) => p.potential as f64,
            None => return,
        };
        let mut rng =
            StdRng::seed_from_u64(self.seed ^ 0x5C0_u64 ^ (pid as u64) ^ ((remaining as u64) << 8));
        if let Some(d) = self.draft.as_mut() {
            let Some(entry) = d.scouting.get_mut(&pid) else { return };
            // Already fully scouted — don't waste a point.
            if entry.confidence() >= 3 {
                return;
            }
            entry.uncertainty *= 0.55;
            let noise = (rng.gen::<f64>() + rng.gen::<f64>() - 1.0) * entry.uncertainty;
            entry.estimate = (pot + noise).clamp(30.0, 99.0);
            d.scout_points -= 1;
        }
    }

    /// Round-1 order: a weighted lottery among the 16 non-playoff teams (worst
    /// record = best odds), followed by the 16 playoff teams worst-record-first.
    fn draft_order(&self, rng: &mut impl Rng) -> Vec<TeamId> {
        let playoff_teams: std::collections::HashSet<TeamId> =
            playoff_seeds(&self.teams, Conference::East)
                .into_iter()
                .chain(playoff_seeds(&self.teams, Conference::West))
                .collect();

        // All teams worst-record-first.
        let mut by_record: Vec<&Team> = self.teams.iter().collect();
        by_record.sort_by(|a, b| {
            a.win_pct().partial_cmp(&b.win_pct()).unwrap().then(a.wins.cmp(&b.wins))
        });

        // Lottery pool = non-playoff teams; weight worst teams highest.
        let mut pool: Vec<TeamId> = by_record
            .iter()
            .filter(|t| !playoff_teams.contains(&t.id))
            .map(|t| t.id)
            .collect();
        let mut weights: Vec<f64> = (0..pool.len()).map(|i| (pool.len() - i) as f64).collect();

        let mut lottery = Vec::with_capacity(pool.len());
        while !pool.is_empty() {
            let total: f64 = weights.iter().sum();
            let mut r = rng.gen::<f64>() * total;
            let mut idx = 0;
            for (i, w) in weights.iter().enumerate() {
                r -= *w;
                if r <= 0.0 {
                    idx = i;
                    break;
                }
            }
            lottery.push(pool.remove(idx));
            weights.remove(idx);
        }

        // Playoff teams worst-record-first slot in after the lottery.
        let playoff_order: Vec<TeamId> = by_record
            .iter()
            .filter(|t| playoff_teams.contains(&t.id))
            .map(|t| t.id)
            .collect();

        lottery.into_iter().chain(playoff_order).collect()
    }

    /// Make the pick currently on the clock for `player_id` and advance.
    fn apply_pick(&mut self, player_id: PlayerId) {
        let (idx, team_id) = match &self.draft {
            Some(d) if !d.is_complete() => (d.on_clock, d.picks[d.on_clock].team_id),
            _ => return,
        };
        if let Some(d) = self.draft.as_mut() {
            d.picks[idx].player_id = Some(player_id);
            d.prospects.retain(|p| *p != player_id);
            d.on_clock += 1;
        }
        let season = self.season;
        if let Some(p) = self.players.iter_mut().find(|p| p.id == player_id) {
            p.team = Some(team_id);
            p.draft_season = Some(season);
        }
        if let Some(t) = self.teams.iter_mut().find(|t| t.id == team_id) {
            t.roster.push(player_id);
        }
    }

    /// Best available prospect (top few by overall, lightly randomized).
    fn cpu_choice(&self, rng: &mut impl Rng) -> Option<PlayerId> {
        let d = self.draft.as_ref()?;
        let mut pool: Vec<&Player> = d
            .prospects
            .iter()
            .filter_map(|id| self.players.iter().find(|p| p.id == *id))
            .collect();
        if pool.is_empty() {
            return None;
        }
        pool.sort_by(|a, b| b.overall().cmp(&a.overall()));
        let top = pool.len().min(3);
        Some(pool[rng.gen_range(0..top)].id)
    }

    pub fn is_user_on_clock(&self) -> bool {
        self.draft
            .as_ref()
            .and_then(|d| d.team_on_clock())
            .map(|t| Some(t) == self.user_team_id)
            .unwrap_or(false)
    }

    pub fn draft_complete(&self) -> bool {
        self.draft.as_ref().map(|d| d.is_complete()).unwrap_or(true)
    }

    /// The user drafts a specific prospect (must be their pick).
    pub fn draft_user_pick(&mut self, player_id: PlayerId) {
        if self.is_user_on_clock() {
            self.apply_pick(player_id);
        }
    }

    /// CPU auto-picks until the user is on the clock or the draft ends.
    pub fn draft_sim_to_user(&mut self) {
        let on = self.draft.as_ref().map(|d| d.on_clock).unwrap_or(0);
        let mut rng = StdRng::seed_from_u64(self.seed ^ 0xC9_u64.wrapping_mul(7) ^ on as u64);
        while !self.draft_complete() && !self.is_user_on_clock() {
            match self.cpu_choice(&mut rng) {
                Some(pid) => self.apply_pick(pid),
                None => break,
            }
        }
    }

    /// CPU auto-picks the entire remaining draft (including the user's slots).
    pub fn draft_sim_all(&mut self) {
        let on = self.draft.as_ref().map(|d| d.on_clock).unwrap_or(0);
        let mut rng = StdRng::seed_from_u64(self.seed ^ 0xA11_u64.wrapping_mul(13) ^ on as u64);
        while !self.draft_complete() {
            match self.cpu_choice(&mut rng) {
                Some(pid) => self.apply_pick(pid),
                None => break,
            }
        }
    }

    // ---- New season ----

    /// Roll into the next season: clear the draft, age players, reset records
    /// and stats, rebuild the schedule, and tip off. Rosters (including drafted
    /// rookies) and customizations are kept.
    pub fn start_new_season(&mut self) {
        self.season += 1;
        self.draft = None;
        // Age and develop every player (young grow toward potential, vets decline).
        let mut dev_rng = StdRng::seed_from_u64(self.seed ^ 0xDE7_u64 ^ (self.season as u64));
        for p in &mut self.players {
            p.age = p.age.saturating_add(1);
            develop_player(p, &mut dev_rng);
        }
        for t in &mut self.teams {
            t.wins = 0;
            t.losses = 0;
        }
        self.season_stats = vec![SeasonStats::default(); self.players.len()];
        self.playoffs = None;
        let team_ids: Vec<TeamId> = self.teams.iter().map(|t| t.id).collect();
        let mut rng = StdRng::seed_from_u64(self.seed ^ 0x5C4ED_u64 ^ (self.season as u64));
        self.schedule = generate_schedule(&team_ids, &mut rng);
        self.phase = Phase::RegularSeason;
    }

    // ---- Save / load ----

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("league serializes")
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}
