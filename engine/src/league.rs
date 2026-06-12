//! The top-level league: all state plus the season state machine.
//!
//! This is the single object the UI holds. It is fully `serde`-serializable, so
//! the web layer can save/load it to the browser's localStorage as JSON.

use crate::draft::{Draft, DraftPick};
use crate::names::{FIRST_NAMES, LAST_NAMES};
use crate::player::{Player, Ratings, SeasonStats};
use crate::playoffs::{simulate_playoffs, Playoffs};
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
    /// The draft, while one is running.
    pub draft: Option<Draft>,
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
            draft: None,
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

        Player {
            id,
            name,
            age: rng.gen_range(19..=38),
            position: pos,
            ratings: gen_ratings(pos, talent, rng),
            team: Some(team_id),
        }
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

    // ---- Playoffs ----

    /// Seed the bracket from final standings and play it out with the full
    /// possession sim.
    pub fn start_playoffs(&mut self) {
        assert!(self.regular_season_complete(), "regular season not finished");
        let east = playoff_seeds(&self.teams, Conference::East);
        let west = playoff_seeds(&self.teams, Conference::West);

        // Map every team to its conference seed (1 = best) for home-court.
        let mut seed_of_map: HashMap<TeamId, u32> = HashMap::new();
        for conf in [Conference::East, Conference::West] {
            for row in conference_standings(&self.teams, conf) {
                seed_of_map.insert(row.team_id, row.seed);
            }
        }
        let seed_of = |id: TeamId| *seed_of_map.get(&id).unwrap_or(&99);

        let mut rng = StdRng::seed_from_u64(self.seed ^ 0x71A04FF_u64);
        let teams = &self.teams;
        let players = &self.players;
        let sim = |home_id: TeamId, away_id: TeamId| -> GameResult {
            let home = teams.iter().find(|t| t.id == home_id).unwrap();
            let away = teams.iter().find(|t| t.id == away_id).unwrap();
            let g = simulate_game(home, away, players, &mut rng);
            GameResult { home_score: g.home.score, away_score: g.away.score }
        };

        let po = simulate_playoffs(&east, &west, seed_of, sim);
        self.playoffs = Some(po);
        self.phase = Phase::Playoffs;
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

    /// Record the finished season into history and move to the offseason.
    pub fn finish_season(&mut self) {
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
            self.players.push(Player {
                id,
                name: format!("{first} {last}"),
                age: rng.gen_range(19..=22),
                position: pos,
                ratings: gen_ratings(pos, talent, &mut rng),
                team: None,
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

        self.draft = Some(Draft { picks, prospects, on_clock: 0 });
        self.phase = Phase::Draft;
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
        if let Some(p) = self.players.iter_mut().find(|p| p.id == player_id) {
            p.team = Some(team_id);
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
        for p in &mut self.players {
            p.age = p.age.saturating_add(1);
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
