//! The top-level league: all state plus the season state machine.
//!
//! This is the single object the UI holds. It is fully `serde`-serializable, so
//! the web layer can save/load it to the browser's localStorage as JSON.

use crate::draft::{Draft, DraftPick, ScoutEntry};
use crate::free_agency::{FaOffer, FreeAgency};
use crate::names::{FIRST_NAMES, LAST_NAMES};
use crate::player::{Career, CareerSeason, Contract, Honor, HonorEntry, Player, Ratings, SeasonStats};
use crate::playoffs::{first_round_pairs, high_seed_hosts, Playoffs, Series};
use crate::schedule::{generate_schedule, Game, GameResult};
use crate::sim::{simulate_game, simulate_game_pbp, PlayEvent, TeamBox};
use crate::standings::{conference_standings, playoff_seeds};
use crate::team::Team;
use crate::teams_data::PRESETS;
use crate::types::{Color, Conference, PlayerId, Position, TeamId};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Generate a player's 16 attributes from a team talent baseline and the
/// position's identity (e.g. centers dunk, post, block and rebound; guards
/// handle, pass and shoot).
fn gen_ratings(pos: Position, talent: f64, rng: &mut impl Rng) -> Ratings {
    let base = talent + rng.gen_range(-12.0..14.0);
    let attr = |modifier: f64, rng: &mut dyn rand::RngCore| -> u8 {
        (base + modifier + rng.gen_range(-8.0..8.0)).clamp(25.0, 99.0) as u8
    };
    // Per-position modifiers, in field order:
    // layup, dunk, post, mid_range, three, free_throw, passing, ball_handling,
    // basketball_iq, interior_defense, perimeter_defense, steal, block,
    // rebounding, athleticism, stamina
    let m: [f64; 16] = match pos {
        Position::PG => [-2., -12., -16., 3., 6., 6., 12., 14., 8., -12., 6., 8., -14., -12., 6., 4.],
        Position::SG => [0., -4., -10., 8., 10., 6., 2., 6., 4., -6., 6., 5., -8., -6., 5., 2.],
        Position::SF => [3., 2., 0., 4., 2., 2., 0., 0., 2., 2., 4., 3., 0., 0., 3., 0.],
        Position::PF => [5., 8., 8., -2., -4., -2., -4., -6., 0., 6., -2., -2., 6., 8., 0., 0.],
        Position::C => [6., 12., 12., -8., -12., -6., -6., -10., 0., 10., -6., -4., 12., 12., -2., -2.],
    };
    Ratings {
        layup: attr(m[0], rng),
        dunk: attr(m[1], rng),
        post: attr(m[2], rng),
        mid_range: attr(m[3], rng),
        three: attr(m[4], rng),
        free_throw: attr(m[5], rng),
        passing: attr(m[6], rng),
        ball_handling: attr(m[7], rng),
        basketball_iq: attr(m[8], rng),
        interior_defense: attr(m[9], rng),
        perimeter_defense: attr(m[10], rng),
        steal: attr(m[11], rng),
        block: attr(m[12], rng),
        rebounding: attr(m[13], rng),
        athleticism: attr(m[14], rng),
        stamina: attr(m[15], rng),
    }
}

/// The league salary cap, in thousands of dollars ($140.0M).
pub const SALARY_CAP: u32 = 140_000;
/// Minimum salary, in thousands ($1.2M).
pub const MIN_SALARY: u32 = 1_200;

/// A fair-market yearly salary (thousands) for a player of the given overall.
/// This is what the player is "worth" — used for generated contracts, free
/// agency demands, and trade-value math.
pub fn market_salary(ovr: u8) -> u32 {
    // Smooth, steep-at-the-top curve. ~45M for a 90, ~12M for a 70, min ~1.2M.
    let o = ovr as f64;
    let raw = if o >= 70.0 {
        12_000.0 + (o - 70.0) * 1_700.0
    } else {
        1_200.0 + (o - 50.0).max(0.0) * 540.0
    };
    (raw.round() as u32).clamp(MIN_SALARY, 48_000)
}

/// Rookie-scale salary (thousands) for a given overall draft pick number.
fn rookie_scale(pick: u8) -> u32 {
    // ~$9.5M for #1 down to the minimum by the end of the draft.
    let top = 9_500.0;
    let v = top - (pick as f64 - 1.0) * 130.0;
    (v.round() as u32).clamp(MIN_SALARY, 9_500)
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
    bump(&mut r.post, rng);
    bump(&mut r.mid_range, rng);
    bump(&mut r.three, rng);
    bump(&mut r.free_throw, rng);
    bump(&mut r.passing, rng);
    bump(&mut r.ball_handling, rng);
    bump(&mut r.basketball_iq, rng);
    bump(&mut r.interior_defense, rng);
    bump(&mut r.perimeter_defense, rng);
    bump(&mut r.steal, rng);
    bump(&mut r.block, rng);
    bump(&mut r.rebounding, rng);
    bump(&mut r.athleticism, rng);
    bump(&mut r.stamina, rng);
}

/// One offseason of development: young players grow toward their potential,
/// veterans decline. Called after ages are incremented.
/// `dev_factor` (~0.8..1.35) comes from the player's team's coaching + training
/// budgets: it scales up growth and softens decline.
fn develop_player(p: &mut Player, rng: &mut impl Rng, dev_factor: f64) {
    let ovr = p.overall();
    if p.age <= 26 && ovr < p.potential {
        let room = (p.potential - ovr) as f64;
        let gain = (room * rng.gen_range(0.20..0.50) * dev_factor).round() as i32;
        let gain = gain.max(1).min((p.potential - ovr) as i32);
        apply_attr_delta(&mut p.ratings, gain, rng);
    } else if p.age >= 31 {
        let severity = (p.age as i32 - 30).max(0);
        // More training (higher factor) slows the decline.
        let loss = ((rng.gen_range(1.0..3.0) + severity as f64 * 0.4) * (2.0 - dev_factor)).round() as i32;
        apply_attr_delta(&mut p.ratings, -loss.max(0), rng);
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

/// How interested a free agent is in the user's current offer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interest {
    NoOffer,
    Unlikely,
    Lukewarm,
    Interested,
    Eager,
}

impl Interest {
    pub fn label(&self) -> &'static str {
        match self {
            Interest::NoOffer => "\u{2014}",
            Interest::Unlikely => "Unlikely",
            Interest::Lukewarm => "Lukewarm",
            Interest::Interested => "Interested",
            Interest::Eager => "Eager",
        }
    }
}

/// A trade the CPU would accept (for the trade finder).
#[derive(Debug, Clone)]
pub struct TradeSuggestion {
    pub other: TeamId,
    pub get: Vec<PlayerId>,
    pub get_value: f64,
    pub message: String,
}

/// A tradeable future draft pick. `season` is the draft year it conveys in,
/// `original` is the team whose slot it is (drives its value), and `owner` is
/// who currently holds it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OwnedPick {
    pub id: u32,
    pub season: u32,
    pub round: u8,
    pub original: TeamId,
    pub owner: TeamId,
}

/// A concrete trade package surfaced by the 2K-style trade finder: exactly what
/// each side gives up (players + picks) and the value math.
#[derive(Debug, Clone)]
pub struct TradePackage {
    pub other: TeamId,
    pub give_players: Vec<PlayerId>,
    pub give_picks: Vec<u32>,
    pub get_players: Vec<PlayerId>,
    pub get_picks: Vec<u32>,
    pub give_value: f64,
    pub get_value: f64,
}

/// A team's projected/booked season profit-and-loss (all money in thousands).
#[derive(Debug, Clone)]
pub struct FinanceProjection {
    pub capacity: u32,
    /// Average attendance per home game.
    pub attendance: u32,
    /// Persistent fan interest (0..1), shown as a % bar.
    pub fan_interest: f64,
    pub stadium_age: u32,
    /// True when demand exceeds capacity (turning fans away → expand).
    pub unmet_demand: bool,
    pub ticket_rev: u32,
    pub concession_rev: u32,
    pub tv_rev: u32,
    pub merch_rev: u32,
    pub revenue: u32,
    pub payroll: u32,
    /// Sum of the four department budgets.
    pub budgets: u32,
    pub expenses: u32,
    /// The owner's soft budget ceiling for the season.
    pub budget: u32,
    pub profit: i64,
}

/// The result of evaluating a proposed trade (for previewing in the UI).
#[derive(Debug, Clone)]
pub struct TradeEval {
    /// Salary-cap and roster legal.
    pub legal: bool,
    /// The CPU would accept it.
    pub accepted: bool,
    pub give_salary: u32,
    pub get_salary: u32,
    pub give_value: f64,
    pub get_value: f64,
    pub message: String,
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
    /// The offseason free-agency period is running.
    FreeAgency,
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
    /// Free agency, while it is running.
    pub free_agency: Option<FreeAgency>,
    /// Awards from the most recently completed season.
    pub awards: Option<Awards>,
    /// The owner's message after the most recently completed season.
    pub owner_message: Option<OwnerMessage>,
    /// Per-player career logs (season-by-season stats + honors), keyed by id.
    #[serde(default)]
    pub careers: HashMap<PlayerId, Career>,
    /// Tradeable future draft picks (a rolling window of the next few drafts).
    #[serde(default)]
    pub pick_assets: Vec<OwnedPick>,
    #[serde(default)]
    next_pick_id: u32,
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
            free_agency: None,
            awards: None,
            owner_message: None,
            careers: HashMap::new(),
            pick_assets: Vec::new(),
            next_pick_id: 0,
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
                market: 1.0,
                finances: crate::team::Finances::default(),
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
        // Give each team a market size (drives TV money + starting arena size)
        // and a starting fan interest.
        for t in &mut league.teams {
            t.market = rng.gen_range(0.80..1.30);
            t.finances.capacity = (15_000.0 + t.market * 5_000.0).round() as u32;
            t.finances.stadium_age = rng.gen_range(2..25);
            t.finances.fan_interest = rng.gen_range(0.35..0.65);
        }

        // Size the season-stats table to match the generated players.
        league.season_stats = vec![SeasonStats::default(); league.players.len()];

        // Seed the rolling window of tradeable future draft picks.
        league.ensure_pick_assets();

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
        // Initial contracts: market value with a little spread, 1–4 years left.
        let salary = (market_salary(ratings.overall()) as f64 * rng.gen_range(0.85..1.15)).round() as u32;
        let contract = Contract { salary: salary.clamp(MIN_SALARY, 48_000), years: rng.gen_range(1..=4) };
        Player { id, name, age, position: pos, ratings, potential, team: Some(team_id), draft_season: None, contract }
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

    /// Play a single scheduled game with a play-by-play feed (for the simcast),
    /// applying its result and stats. Returns the events, or `None` if the game
    /// index is invalid or already played.
    pub fn watch_scheduled_game(&mut self, game_idx: usize) -> Option<Vec<PlayEvent>> {
        let g = self.schedule.get(game_idx)?;
        if g.is_played() {
            return None;
        }
        let (home_id, away_id) = (g.home, g.away);
        let mut rng = StdRng::seed_from_u64(self.seed.wrapping_mul(7919).wrapping_add(game_idx as u64));
        let home = self.teams.iter().find(|t| t.id == home_id)?;
        let away = self.teams.iter().find(|t| t.id == away_id)?;
        let (sim, events) = simulate_game_pbp(home, away, &self.players, &mut rng);

        accumulate_stats(&mut self.season_stats, &sim.home);
        accumulate_stats(&mut self.season_stats, &sim.away);
        let res = GameResult { home_score: sim.home.score, away_score: sim.away.score };
        let home_won = res.home_won();
        if let Some(t) = self.teams.iter_mut().find(|t| t.id == home_id) {
            if home_won { t.wins += 1 } else { t.losses += 1 }
        }
        if let Some(t) = self.teams.iter_mut().find(|t| t.id == away_id) {
            if home_won { t.losses += 1 } else { t.wins += 1 }
        }
        self.schedule[game_idx].result = Some(res);
        Some(events)
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

    /// Sim the rest of the current round. If the deepest round is already
    /// decided (e.g. you just finished one), the first game-day advances the
    /// bracket to the next round and plays it out — so pressing "Sim Round"
    /// repeatedly walks round by round without needing "Sim Game" in between.
    pub fn playoff_sim_round(&mut self) {
        if self.playoffs.is_none() || self.playoffs_complete() {
            return;
        }
        loop {
            // Always make progress: this plays a game-day, first advancing the
            // bracket if the current round was already decided.
            if !self.playoff_sim_gameday() {
                break;
            }
            if self.playoffs_complete() || self.current_round_decided() {
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

    /// Total payroll (thousands) for a team: the sum of its roster's salaries.
    pub fn team_payroll(&self, tid: TeamId) -> u32 {
        let Some(team) = self.teams.iter().find(|t| t.id == tid) else { return 0 };
        team.roster
            .iter()
            .filter_map(|pid| self.players.iter().find(|p| p.id == *pid))
            .map(|p| p.contract.salary)
            .sum()
    }

    /// Cap room (can be negative) for a team.
    pub fn team_cap_space(&self, tid: TeamId) -> i64 {
        SALARY_CAP as i64 - self.team_payroll(tid) as i64
    }

    /// Decrement every contract by a year and release any that expire to the
    /// free-agent pool. Called at season's end.
    fn process_contracts(&mut self) {
        let mut released: Vec<PlayerId> = Vec::new();
        for p in &mut self.players {
            if p.team.is_some() && p.contract.years > 0 {
                p.contract.years -= 1;
                if p.contract.years == 0 {
                    released.push(p.id);
                }
            }
        }
        for pid in released {
            if let Some(p) = self.players.iter_mut().find(|p| p.id == pid) {
                let team = p.team.take();
                p.contract = Contract::free_agent();
                if let Some(tid) = team {
                    if let Some(t) = self.teams.iter_mut().find(|t| t.id == tid) {
                        t.roster.retain(|x| *x != pid);
                    }
                }
            }
        }
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
                let score = stl * 3.0 + blk * 3.0 + s.rpg() * 0.7 + p.ratings.defending() as f64 * 0.15;
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

        // Expectation vs result.
        let exp_word = if win_diff >= 6 { "blew past" }
            else if win_diff >= 1 { "edged past" }
            else if win_diff >= -5 { "came up just shy of" }
            else { "fell well short of" };
        let mut body = format!("At {record}, you {exp_word} the {expected_wins} wins I projected for this roster.");

        // Year-over-year.
        if let Some(h) = self.history.last() {
            let d = team.wins as i32 - h.user_wins as i32;
            if d > 3 {
                body.push_str(&format!(" That's a real step up from last year's {}-{}.", h.user_wins, h.user_losses));
            } else if d < -3 {
                body.push_str(&format!(" We slid back from last year's {}-{}.", h.user_wins, h.user_losses));
            } else {
                body.push_str(&format!(" Right about where we landed last year ({}-{}).", h.user_wins, h.user_losses));
            }
        }

        // Postseason.
        body.push_str(match self.outcome_for(uid) {
            PlayoffOutcome::MissedPlayoffs => " Watching the playoffs from home stings.",
            PlayoffOutcome::LostInRound(0) => " A first-round exit isn't this group's ceiling.",
            PlayoffOutcome::LostInRound(1) => " Bowing out in the second round left meat on the bone.",
            PlayoffOutcome::LostInRound(2) => " A conference-finals run was a genuine step forward.",
            PlayoffOutcome::LostInRound(3) => " So close in the Finals — I can taste it.",
            _ => "",
        });

        // A stat-driven observation.
        let (pf, pa) = self.team_points(uid);
        let lppg = self.league_ppg();
        if pa - lppg > 3.0 {
            body.push_str(" We gave up far too many points on the defensive end.");
        } else if lppg - pf > 3.0 {
            body.push_str(" The offense went cold too often for my liking.");
        } else if let Some((name, ppg)) = team.roster.iter()
            .filter_map(|pid| self.players.iter().find(|p| p.id == *pid))
            .map(|p| (p.name.clone(), self.season_stats[p.id as usize].ppg()))
            .filter(|(_, ppg)| *ppg > 0.0)
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
        {
            body.push_str(&format!(" {} leading us at {:.1} a night was a bright spot.", name, ppg));
        }

        // Business side: profit, fan interest, and the arena situation.
        let fp = self.project_finances(uid);
        let fin = &team.finances;
        if fp.profit < -25_000 {
            body.push_str(&format!(" On the business side we bled ${:.0}M this year — I can't keep covering losses like that.", (-fp.profit) as f64 / 1000.0));
        } else if fp.profit > 45_000 {
            body.push_str(&format!(" The books look terrific too — we cleared ${:.0}M.", fp.profit as f64 / 1000.0));
        }
        if fp.unmet_demand {
            body.push_str(&format!(" And we're selling out {} every night and still turning fans away — it's time we expanded the arena.", fin.capacity));
        } else if fin.fan_interest < 0.32 {
            body.push_str(" The fans are tuning out; we need to give this city something to believe in.");
        } else if fin.fan_interest > 0.82 {
            body.push_str(" The city is electric — I've never seen fan interest this high.");
        }
        if fin.stadium_age >= 25 {
            body.push_str(&format!(" One more thing: the building is {} years old and showing it — we should look at a renovation.", fin.stadium_age));
        }

        // The ask, or warm words.
        match &goal {
            Some(g) => body.push_str(&format!(" Next season, I want you to {g}.")),
            None => body.push_str(match tone {
                OwnerTone::Pleased => " Keep it up, my son.",
                _ => " Let's keep building on this.",
            }),
        }

        OwnerMessage { tone, body }
    }

    /// Record the finished season into history and move to the offseason.
    pub fn finish_season(&mut self) {
        // Book finances first so the owner's note can speak to this season's
        // profit, fan interest, and arena situation.
        self.commit_finances();

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
        // Log this season into every player's career (stat line + honors), so
        // player detail views can show accomplishments over time.
        self.record_careers();

        // Tick down contracts; expiring players hit the free-agent pool.
        self.process_contracts();
        self.phase = Phase::Offseason;
    }

    /// Append this season's stat line and any honors won to each player's career
    /// log. Called once per season, at `finish_season`, after awards are set.
    fn record_careers(&mut self) {
        let season = self.season;
        let awards = self.awards.clone().unwrap_or_default();
        let champ = self.playoffs.as_ref().and_then(|p| p.champion);
        let fmvp = self.playoffs.as_ref().and_then(|p| p.finals_mvp);
        let abbrevs: HashMap<TeamId, String> =
            self.teams.iter().map(|t| (t.id, t.abbrev.clone())).collect();

        // Gather updates under immutable borrows, then apply them.
        let mut updates: Vec<(PlayerId, Option<CareerSeason>, Vec<Honor>)> = Vec::new();
        for p in &self.players {
            let st = &self.season_stats[p.id as usize];
            let mut honors = Vec::new();
            if awards.mvp == Some(p.id) { honors.push(Honor::Mvp); }
            if awards.dpoy == Some(p.id) { honors.push(Honor::Dpoy); }
            if awards.roy == Some(p.id) { honors.push(Honor::Roy); }
            if fmvp == Some(p.id) { honors.push(Honor::FinalsMvp); }
            if champ.is_some() && p.team == champ { honors.push(Honor::Champion); }
            let season_row = if st.gp > 0 {
                Some(CareerSeason {
                    season,
                    team_abbrev: p.team.and_then(|t| abbrevs.get(&t).cloned()).unwrap_or_default(),
                    age: p.age,
                    overall: p.overall(),
                    stats: st.clone(),
                })
            } else {
                None
            };
            if season_row.is_some() || !honors.is_empty() {
                updates.push((p.id, season_row, honors));
            }
        }
        for (pid, row, honors) in updates {
            let c = self.careers.entry(pid).or_default();
            if let Some(row) = row {
                c.seasons.push(row);
            }
            for honor in honors {
                c.honors.push(HonorEntry { season, honor });
            }
        }
    }

    /// A player's career log, if they have any recorded seasons or honors.
    pub fn career(&self, pid: PlayerId) -> Option<&Career> {
        self.careers.get(&pid)
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
                contract: Contract::free_agent(),
            });
            prospects.push(id);
        }
        // Keep the season-stats table sized to the player list.
        self.season_stats.resize(self.players.len(), SeasonStats::default());

        // --- Draft order --- (traded picks convey to their current owner)
        let order = self.draft_order(&mut rng);
        let mut picks = Vec::with_capacity(64);
        for round in 1..=2u8 {
            for (i, &team_id) in order.iter().enumerate() {
                // Who actually holds this slot's pick this year?
                let owner = self
                    .pick_assets
                    .iter()
                    .find(|pk| pk.season == self.season && pk.round == round && pk.original == team_id)
                    .map(|pk| pk.owner)
                    .unwrap_or(team_id);
                picks.push(DraftPick {
                    round,
                    overall: ((round as usize - 1) * 32 + i + 1) as u8,
                    team_id: owner,
                    player_id: None,
                });
            }
        }
        // This draft's pick assets are now spent.
        let drafted_season = self.season;
        self.pick_assets.retain(|pk| pk.season != drafted_season);

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
        let (idx, team_id, pick_no) = match &self.draft {
            Some(d) if !d.is_complete() => (d.on_clock, d.picks[d.on_clock].team_id, d.picks[d.on_clock].overall),
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
            // Rookies sign a cheap 3-year scale deal by draft slot.
            p.contract = Contract { salary: rookie_scale(pick_no), years: 3 };
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

    // ---- Trades ----

    /// Day after which in-season trades are no longer allowed.
    const TRADE_DEADLINE_DAY: u32 = 50;

    /// Can a trade be made right now? (In-season before the deadline, or in the
    /// offseason.)
    pub fn can_trade(&self) -> bool {
        match self.phase {
            Phase::RegularSeason => self.current_day().map(|d| d < Self::TRADE_DEADLINE_DAY).unwrap_or(false),
            Phase::Offseason => true,
            _ => false,
        }
    }

    /// A player's value as a trade asset (production, upside, age, contract).
    pub fn player_trade_value(&self, pid: PlayerId) -> f64 {
        let Some(p) = self.players.iter().find(|p| p.id == pid) else { return 0.0 };
        let ovr = p.overall() as f64;
        let base = (ovr - 40.0).max(0.0).powf(1.9);
        let upside = if p.age < 25 { (p.potential as f64 - ovr).max(0.0) * 6.0 } else { 0.0 };
        let age_pen = (p.age as f64 - 29.0).max(0.0) * 18.0;
        // Being paid above market hurts value; a bargain deal helps it.
        let burden = (p.contract.salary as f64 - market_salary(p.overall()) as f64) * 0.015;
        (base + upside - age_pen - burden).max(0.0)
    }

    fn sum_salary(&self, ids: &[PlayerId]) -> u32 {
        ids.iter()
            .filter_map(|pid| self.players.iter().find(|p| p.id == *pid))
            .map(|p| p.contract.salary)
            .sum()
    }

    fn sum_value(&self, ids: &[PlayerId]) -> f64 {
        ids.iter().map(|pid| self.player_trade_value(*pid)).sum()
    }

    /// Is a team's incoming salary legal given what it sends out? (Either it
    /// ends under the cap, or salaries match within ~25%.)
    fn salary_legal(&self, post_payroll: i64, incoming: u32, outgoing: u32) -> bool {
        post_payroll <= SALARY_CAP as i64 || incoming as f64 <= outgoing as f64 * 1.25 + 1_000.0
    }

    /// Evaluate (without executing) a player-only proposed trade.
    pub fn evaluate_trade(&self, other: TeamId, give: &[PlayerId], get: &[PlayerId]) -> TradeEval {
        self.evaluate_trade_full(other, give, &[], get, &[])
    }

    /// Evaluate a proposed trade including draft picks: the user sends
    /// `give_players` + `give_picks` and receives `get_players` + `get_picks`.
    pub fn evaluate_trade_full(
        &self,
        other: TeamId,
        give_players: &[PlayerId],
        give_picks: &[u32],
        get_players: &[PlayerId],
        get_picks: &[u32],
    ) -> TradeEval {
        let give = give_players;
        let get = get_players;
        let give_pick_val: f64 = give_picks.iter().filter_map(|id| self.pick_by_id(*id)).map(|pk| self.pick_value(pk)).sum();
        let get_pick_val: f64 = get_picks.iter().filter_map(|id| self.pick_by_id(*id)).map(|pk| self.pick_value(pk)).sum();
        let mut eval = TradeEval {
            legal: false,
            accepted: false,
            give_salary: self.sum_salary(give),
            get_salary: self.sum_salary(get),
            give_value: self.sum_value(give) + give_pick_val,
            get_value: self.sum_value(get) + get_pick_val,
            message: String::new(),
        };
        let Some(user) = self.user_team_id else {
            eval.message = "No team selected.".into();
            return eval;
        };
        if give.is_empty() && get.is_empty() && give_picks.is_empty() && get_picks.is_empty() {
            eval.message = "Add assets to both sides.".into();
            return eval;
        }

        // Roster sizes after the swap (picks don't count against the roster).
        let user_size = self.roster_len(user) as i64 - give.len() as i64 + get.len() as i64;
        let other_size = self.roster_len(other) as i64 - get.len() as i64 + give.len() as i64;
        if user_size > Self::ROSTER_MAX as i64 || other_size > Self::ROSTER_MAX as i64 {
            eval.message = "That would put a team over the 15-man roster limit.".into();
            return eval;
        }
        if user_size < 1 || other_size < 1 {
            eval.message = "Both teams must keep at least one player.".into();
            return eval;
        }

        // Salary legality for both teams.
        let user_post = self.team_payroll(user) as i64 - eval.give_salary as i64 + eval.get_salary as i64;
        let other_post = self.team_payroll(other) as i64 - eval.get_salary as i64 + eval.give_salary as i64;
        let user_ok = self.salary_legal(user_post, eval.get_salary, eval.give_salary);
        let other_ok = self.salary_legal(other_post, eval.give_salary, eval.get_salary);
        if !user_ok || !other_ok {
            eval.message = "Salaries don't match (must be within ~25% unless under the cap).".into();
            return eval;
        }
        eval.legal = true;

        // The CPU accepts only if it comes out ahead on value.
        let other_gains = eval.give_value - eval.get_value;
        if other_gains >= eval.get_value * 0.05 + 20.0 {
            eval.accepted = true;
            eval.message = "Accepted! They like this deal.".into();
        } else if other_gains >= -20.0 {
            eval.message = "They're close — sweeten it a little.".into();
        } else {
            eval.message = "Rejected. They want more value coming back.".into();
        }
        eval
    }

    /// Find single-player returns the CPU would accept for the given player you
    /// put on the block. Best returns first.
    pub fn find_trades_for(&self, give_pid: PlayerId) -> Vec<TradeSuggestion> {
        let Some(user) = self.user_team_id else { return Vec::new() };
        if self.players.iter().find(|p| p.id == give_pid).and_then(|p| p.team) != Some(user) {
            return Vec::new();
        }
        let mut out = Vec::new();
        for team in &self.teams {
            if team.id == user {
                continue;
            }
            for &rid in &team.roster {
                let eval = self.evaluate_trade(team.id, &[give_pid], &[rid]);
                if eval.legal && eval.accepted {
                    let name = self.players.iter().find(|p| p.id == rid).map(|p| p.name.clone()).unwrap_or_default();
                    out.push(TradeSuggestion {
                        other: team.id,
                        get: vec![rid],
                        get_value: eval.get_value,
                        message: format!("{} ({})", name, team.abbrev),
                    });
                }
            }
        }
        // Best value coming back first.
        out.sort_by(|a, b| b.get_value.partial_cmp(&a.get_value).unwrap());
        out.truncate(12);
        out
    }

    /// Execute a player-only trade if legal and accepted.
    pub fn execute_trade(&mut self, other: TeamId, give: &[PlayerId], get: &[PlayerId]) -> bool {
        self.execute_trade_full(other, give, &[], get, &[])
    }

    /// Execute a trade including draft picks if it is legal and the CPU accepts.
    pub fn execute_trade_full(
        &mut self,
        other: TeamId,
        give_players: &[PlayerId],
        give_picks: &[u32],
        get_players: &[PlayerId],
        get_picks: &[u32],
    ) -> bool {
        let Some(user) = self.user_team_id else { return false };
        // Validate player ownership.
        let give_ok = give_players.iter().all(|pid| self.players.iter().any(|p| p.id == *pid && p.team == Some(user)));
        let get_ok = get_players.iter().all(|pid| self.players.iter().any(|p| p.id == *pid && p.team == Some(other)));
        // Validate pick ownership.
        let gp_ok = give_picks.iter().all(|id| self.pick_by_id(*id).map(|pk| pk.owner == user).unwrap_or(false));
        let rp_ok = get_picks.iter().all(|id| self.pick_by_id(*id).map(|pk| pk.owner == other).unwrap_or(false));
        if !give_ok || !get_ok || !gp_ok || !rp_ok {
            return false;
        }
        let eval = self.evaluate_trade_full(other, give_players, give_picks, get_players, get_picks);
        if !eval.legal || !eval.accepted {
            return false;
        }
        // Move players.
        for &pid in give_players {
            if let Some(p) = self.players.iter_mut().find(|p| p.id == pid) {
                p.team = Some(other);
            }
        }
        for &pid in get_players {
            if let Some(p) = self.players.iter_mut().find(|p| p.id == pid) {
                p.team = Some(user);
            }
        }
        if let Some(t) = self.teams.iter_mut().find(|t| t.id == user) {
            t.roster.retain(|x| !give_players.contains(x));
            t.roster.extend(get_players.iter().copied());
        }
        if let Some(t) = self.teams.iter_mut().find(|t| t.id == other) {
            t.roster.retain(|x| !get_players.contains(x));
            t.roster.extend(give_players.iter().copied());
        }
        // Reassign picks.
        for id in give_picks {
            if let Some(pk) = self.pick_assets.iter_mut().find(|pk| pk.id == *id) {
                pk.owner = other;
            }
        }
        for id in get_picks {
            if let Some(pk) = self.pick_assets.iter_mut().find(|pk| pk.id == *id) {
                pk.owner = user;
            }
        }
        true
    }

    // ---- Draft-pick assets ----

    /// How many future drafts are tradeable at once (a rolling window).
    const PICK_YEARS: u32 = 4;

    fn pick_by_id(&self, id: u32) -> Option<&OwnedPick> {
        self.pick_assets.iter().find(|pk| pk.id == id)
    }

    /// All picks currently owned by a team, soonest first.
    pub fn picks_owned_by(&self, tid: TeamId) -> Vec<&OwnedPick> {
        let mut v: Vec<&OwnedPick> = self.pick_assets.iter().filter(|pk| pk.owner == tid).collect();
        v.sort_by(|a, b| a.season.cmp(&b.season).then(a.round.cmp(&b.round)));
        v
    }

    /// A short label for a pick, e.g. "2027 1st (CIN)".
    pub fn pick_label(&self, id: u32) -> String {
        match self.pick_by_id(id) {
            None => "pick".into(),
            Some(pk) => {
                let rd = if pk.round == 1 { "1st" } else { "2nd" };
                let orig = self.teams.iter().find(|t| t.id == pk.original).map(|t| t.abbrev.clone()).unwrap_or_default();
                format!("{} {} ({})", pk.season, rd, orig)
            }
        }
    }

    /// A draft pick's trade value, from the original team's projected strength
    /// (weaker team → higher pick → more valuable) and how far out it is.
    pub fn pick_value(&self, pk: &OwnedPick) -> f64 {
        let strength = |tid: TeamId| self.teams.iter().find(|t| t.id == tid).map(|t| t.strength(&self.players)).unwrap_or(60.0);
        let league_avg = self.teams.iter().map(|t| t.strength(&self.players)).sum::<f64>() / self.teams.len().max(1) as f64;
        let badness = ((league_avg - strength(pk.original)) / 15.0).clamp(-1.0, 1.0);
        let (base, swing, floor) = if pk.round == 1 { (520.0, 300.0, 170.0) } else { (110.0, 60.0, 30.0) };
        let raw = base + swing * badness;
        let years_out = pk.season.saturating_sub(self.season);
        let discount = 1.0 / (1.0 + 0.10 * years_out as f64);
        (raw * discount).max(floor)
    }

    /// Make sure every team owns its own 1st and 2nd for each draft in the
    /// rolling window `[season ..= season + PICK_YEARS - 1]`. Never disturbs
    /// picks that have been traded away.
    fn ensure_pick_assets(&mut self) {
        let team_ids: Vec<TeamId> = self.teams.iter().map(|t| t.id).collect();
        // Drop any stale picks for drafts that already happened.
        let season = self.season;
        self.pick_assets.retain(|pk| pk.season >= season);
        for yr in season..season + Self::PICK_YEARS {
            for &tid in &team_ids {
                for round in 1..=2u8 {
                    let exists = self.pick_assets.iter().any(|pk| pk.season == yr && pk.round == round && pk.original == tid);
                    if !exists {
                        let id = self.next_pick_id;
                        self.next_pick_id += 1;
                        self.pick_assets.push(OwnedPick { id, season: yr, round, original: tid, owner: tid });
                    }
                }
            }
        }
    }

    /// 2K-style "acquire" finder: packages of YOUR assets that land the target
    /// player from his team. Cheapest (best-value-for-you) first.
    pub fn find_packages_to_acquire(&self, target: PlayerId) -> Vec<TradePackage> {
        let Some(user) = self.user_team_id else { return Vec::new() };
        let Some(tp) = self.players.iter().find(|p| p.id == target) else { return Vec::new() };
        let Some(other) = tp.team else { return Vec::new() };
        if other == user {
            return Vec::new();
        }

        // Candidate assets we can send: our players (by value asc) + our picks.
        let mut my_players: Vec<PlayerId> = self.teams.iter().find(|t| t.id == user).map(|t| t.roster.clone()).unwrap_or_default();
        my_players.sort_by(|a, b| self.player_trade_value(*a).partial_cmp(&self.player_trade_value(*b)).unwrap());
        let my_picks: Vec<u32> = self.picks_owned_by(user).iter().map(|pk| pk.id).collect();

        let get = [target];
        let mut out: Vec<TradePackage> = Vec::new();
        let n = my_players.len();
        // Try 1–3 players, optionally plus one pick as a sweetener.
        for i in 0..n {
            for j in i..n {
                for k in j..n {
                    let mut base: Vec<PlayerId> = vec![my_players[i]];
                    if j != i { base.push(my_players[j]); }
                    if k != j && k != i { base.push(my_players[k]); }
                    base.sort_unstable();
                    base.dedup();
                    let pick_opts: Vec<Vec<u32>> = std::iter::once(vec![])
                        .chain(my_picks.iter().map(|p| vec![*p]))
                        .collect();
                    for gp in &pick_opts {
                        let e = self.evaluate_trade_full(other, &base, gp, &get, &[]);
                        if e.legal && e.accepted {
                            out.push(TradePackage {
                                other,
                                give_players: base.clone(),
                                give_picks: gp.clone(),
                                get_players: get.to_vec(),
                                get_picks: vec![],
                                give_value: e.give_value,
                                get_value: e.get_value,
                            });
                        }
                    }
                }
            }
        }
        // Cheapest legit package first; keep a spread.
        out.sort_by(|a, b| a.give_value.partial_cmp(&b.give_value).unwrap());
        out.truncate(6);
        out
    }

    /// 2K-style "trade away" finder: for a player you shop, the best package each
    /// other team would give up (players + picks). Best return first.
    pub fn find_packages_to_trade_away(&self, shop: PlayerId) -> Vec<TradePackage> {
        let Some(user) = self.user_team_id else { return Vec::new() };
        if self.players.iter().find(|p| p.id == shop).and_then(|p| p.team) != Some(user) {
            return Vec::new();
        }
        let give = [shop];
        let mut out: Vec<TradePackage> = Vec::new();
        for team in &self.teams {
            if team.id == user {
                continue;
            }
            // Their assets: top players by value + their picks.
            let mut their: Vec<PlayerId> = team.roster.clone();
            their.sort_by(|a, b| self.player_trade_value(*b).partial_cmp(&self.player_trade_value(*a)).unwrap());
            their.truncate(10);
            let their_picks: Vec<u32> = self.picks_owned_by(team.id).iter().map(|pk| pk.id).collect();

            let mut best: Option<TradePackage> = None;
            let m = their.len();
            for i in 0..m {
                for j in i..m {
                    let mut base: Vec<PlayerId> = vec![their[i]];
                    if j != i { base.push(their[j]); }
                    let pick_opts: Vec<Vec<u32>> = std::iter::once(vec![])
                        .chain(their_picks.iter().map(|p| vec![*p]))
                        .collect();
                    for rp in &pick_opts {
                        let e = self.evaluate_trade_full(team.id, &give, &[], &base, rp);
                        if e.legal && e.accepted {
                            // Best = most value coming back to the user.
                            if best.as_ref().map(|b| e.get_value > b.get_value).unwrap_or(true) {
                                best = Some(TradePackage {
                                    other: team.id,
                                    give_players: give.to_vec(),
                                    give_picks: vec![],
                                    get_players: base.clone(),
                                    get_picks: rp.clone(),
                                    give_value: e.give_value,
                                    get_value: e.get_value,
                                });
                            }
                        }
                    }
                }
            }
            if let Some(b) = best {
                out.push(b);
            }
        }
        out.sort_by(|a, b| b.get_value.partial_cmp(&a.get_value).unwrap());
        out.truncate(10);
        out
    }

    // ---- Finances ----

    /// Home games in a season (half of 82).
    const HOME_GAMES: u32 = 41;

    /// Development speedup from a team's coaching + training budgets (1.0 at the
    /// default $16M combined; up to ~1.35 heavily funded, down to 0.8 gutted).
    fn team_dev_factor(&self, tid: TeamId) -> f64 {
        let Some(t) = self.teams.iter().find(|t| t.id == tid) else { return 1.0 };
        let sum_m = (t.finances.coaching + t.finances.training) as f64 / 1000.0;
        (1.0 + (sum_m - 16.0) * 0.02).clamp(0.8, 1.35)
    }

    /// A team's extra pull in free agency from nice facilities + engaged fans.
    fn team_fa_bonus(&self, tid: TeamId) -> f64 {
        let Some(t) = self.teams.iter().find(|t| t.id == tid) else { return 0.0 };
        let fac_m = t.finances.facilities as f64 / 1000.0;
        (fac_m - 8.0) / 8.0 * 0.06 + t.finances.fan_interest * 0.05
    }

    /// A single player's jersey sales this season: units + revenue (thousands).
    /// Driven by star power, scoring, and the team's fan interest + market.
    pub fn player_jersey_sales(&self, pid: PlayerId) -> (u32, u32) {
        let Some(p) = self.players.iter().find(|p| p.id == pid) else { return (0, 0) };
        let Some(tid) = p.team else { return (0, 0) };
        let Some(t) = self.teams.iter().find(|t| t.id == tid) else { return (0, 0) };
        let ovr = p.overall() as f64;
        let ppg = self.season_stats.get(pid as usize).map(|s| s.ppg()).unwrap_or(0.0);
        let star = (ovr - 60.0).max(0.0);
        let base = star.powf(1.7) * 200.0 + ppg * 6_000.0;
        let units = (base * t.finances.fan_interest * t.market).round() as u32;
        let revenue = units * 110 / 1000; // ~$110/jersey, in thousands
        (units, revenue)
    }

    /// Total merch revenue for a team (thousands): player jerseys + team goods.
    fn team_merch_revenue(&self, tid: TeamId) -> u32 {
        let Some(t) = self.teams.iter().find(|t| t.id == tid) else { return 0 };
        let jerseys: u32 = t.roster.iter().map(|pid| self.player_jersey_sales(*pid).1).sum();
        let team_goods = (t.finances.fan_interest * t.market * 9_000.0) as u32;
        jerseys + team_goods
    }

    /// Compute the full P&L for a team given an assumed win pct (thousands).
    fn finance_calc(&self, tid: TeamId, win_pct: f64) -> FinanceProjection {
        let t = self.teams.iter().find(|t| t.id == tid).expect("team exists");
        let f = &t.finances;
        let cap = f.capacity;
        let fac_m = f.facilities as f64 / 1000.0;
        // Uncapped demand as a fraction of capacity — if it exceeds 1.0 the team
        // is turning fans away (a signal to expand the arena).
        let raw_demand = 0.42
            + f.fan_interest * 0.55
            + (win_pct - 0.4) * 0.35
            + (fac_m - 8.0) / 8.0 * 0.05
            - (f.ticket_price as f64 - 55.0) / 55.0 * 0.20
            - (f.stadium_age as f64 - 10.0).max(0.0) / 10.0 * 0.04;
        let unmet_demand = raw_demand > 1.0;
        let demand = raw_demand.clamp(0.30, 1.0);
        let attendance = (cap as f64 * demand) as u32;
        let g = Self::HOME_GAMES as f64;
        let gate = attendance as f64 * f.ticket_price as f64 * g / 1000.0;
        let conc = attendance as f64 * f.concession_price as f64 * 0.6 * g / 1000.0;
        let tv = 65_000.0 + t.market * 25_000.0 + f.fan_interest * 15_000.0;
        let merch = self.team_merch_revenue(tid);
        let revenue = (gate + conc + tv) as u32 + merch;
        let payroll = self.team_payroll(tid);
        let budgets = f.coaching + f.training + f.facilities + f.marketing;
        let expenses = payroll + budgets;
        let budget = revenue + 15_000 + (t.market * 15_000.0) as u32;
        FinanceProjection {
            capacity: cap,
            attendance,
            fan_interest: f.fan_interest,
            stadium_age: f.stadium_age,
            unmet_demand,
            ticket_rev: gate as u32,
            concession_rev: conc as u32,
            tv_rev: tv as u32,
            merch_rev: merch,
            revenue,
            payroll,
            budgets,
            expenses,
            budget,
            profit: revenue as i64 - expenses as i64,
        }
    }

    /// Project a team's finances from its current record (0.5 if none yet).
    pub fn project_finances(&self, tid: TeamId) -> FinanceProjection {
        let wp = self
            .teams
            .iter()
            .find(|t| t.id == tid)
            .map(|t| if t.games_played() > 0 { t.win_pct() } else { 0.5 })
            .unwrap_or(0.5);
        self.finance_calc(tid, wp)
    }

    /// Jersey-sales leaderboard across the whole league (top sellers first):
    /// (player id, name, team abbrev, units, revenue-thousands).
    pub fn league_jersey_leaders(&self, top: usize) -> Vec<(PlayerId, String, String, u32, u32)> {
        let mut rows: Vec<(PlayerId, String, String, u32, u32)> = self
            .players
            .iter()
            .filter(|p| p.team.is_some())
            .map(|p| {
                let (units, rev) = self.player_jersey_sales(p.id);
                let ab = p.team.and_then(|t| self.teams.iter().find(|x| x.id == t)).map(|t| t.abbrev.clone()).unwrap_or_default();
                (p.id, p.name.clone(), ab, units, rev)
            })
            .collect();
        rows.sort_by(|a, b| b.3.cmp(&a.3));
        rows.truncate(top);
        rows
    }

    /// Update the user team's finance settings (prices + department budgets).
    #[allow(clippy::too_many_arguments)]
    pub fn set_user_finances(&mut self, ticket: u32, concession: u32, coaching: u32, training: u32, facilities: u32, marketing: u32) {
        let Some(user) = self.user_team_id else { return };
        if let Some(t) = self.teams.iter_mut().find(|t| t.id == user) {
            let f = &mut t.finances;
            f.ticket_price = ticket.clamp(10, 300);
            f.concession_price = concession.clamp(5, 100);
            f.coaching = coaching.min(40_000);
            f.training = training.min(40_000);
            f.facilities = facilities.min(40_000);
            f.marketing = marketing.min(40_000);
        }
    }

    /// Cost (thousands) of the next arena expansion, owner-funded.
    pub fn stadium_upgrade_cost(&self, tid: TeamId) -> u32 {
        self.teams.iter().find(|t| t.id == tid).map(|t| t.finances.capacity * 12).unwrap_or(0)
    }

    /// The owner only funds an expansion the team has earned — high fan interest
    /// or an arena that's already turning fans away.
    pub fn can_upgrade_stadium(&self, tid: TeamId) -> bool {
        let Some(t) = self.teams.iter().find(|t| t.id == tid) else { return false };
        if t.finances.capacity >= 30_000 {
            return false;
        }
        t.finances.fan_interest >= 0.65 || self.project_finances(tid).unmet_demand
    }

    /// Expand + renovate the user's arena: +3,000 seats and a reset age. Owner
    /// funded, so it doesn't hit the operating budget. Returns success.
    pub fn upgrade_stadium(&mut self) -> bool {
        let Some(user) = self.user_team_id else { return false };
        if !self.can_upgrade_stadium(user) {
            return false;
        }
        if let Some(t) = self.teams.iter_mut().find(|t| t.id == user) {
            t.finances.capacity = (t.finances.capacity + 3_000).min(30_000);
            t.finances.stadium_age = 0;
            return true;
        }
        false
    }

    /// Book the finished season's finances for every team, drift fan interest
    /// toward what the year earned, and age every arena.
    fn commit_finances(&mut self) {
        let playoff_teams: std::collections::HashSet<TeamId> =
            playoff_seeds(&self.teams, Conference::East)
                .into_iter()
                .chain(playoff_seeds(&self.teams, Conference::West))
                .collect();
        let champ = self.playoffs.as_ref().and_then(|p| p.champion);

        // (id, proj, win_pct, made_playoffs, is_champ, best_ovr)
        let rows: Vec<(TeamId, FinanceProjection, f64, bool, bool, u8)> = self
            .teams
            .iter()
            .map(|t| {
                let wp = if t.games_played() > 0 { t.win_pct() } else { 0.5 };
                let best = t.roster.iter().filter_map(|pid| self.players.iter().find(|p| p.id == *pid)).map(|p| p.overall()).max().unwrap_or(0);
                (t.id, self.finance_calc(t.id, wp), wp, playoff_teams.contains(&t.id), champ == Some(t.id), best)
            })
            .collect();

        for (tid, proj, wp, made_po, is_champ, best) in rows {
            let merch = proj.merch_rev;
            if let Some(t) = self.teams.iter_mut().find(|t| t.id == tid) {
                let f = &mut t.finances;
                f.last_attendance = proj.attendance;
                f.last_revenue = proj.revenue;
                f.last_merch = merch;
                f.last_expenses = proj.expenses;
                f.last_profit = proj.profit;
                f.stadium_age = f.stadium_age.saturating_add(1);
                // Fan interest drifts (sticky) toward what the season earned.
                let mkt_m = f.marketing as f64 / 1000.0;
                let star_bonus = ((best as f64 - 80.0) / 15.0).clamp(0.0, 1.0) * 0.12;
                let po_bonus = if is_champ { 0.15 } else if made_po { 0.06 } else { 0.0 };
                let target = (0.28 + wp * 0.40 + po_bonus + star_bonus + (mkt_m - 6.0) / 6.0 * 0.05).clamp(0.05, 1.0);
                f.fan_interest = (f.fan_interest + (target - f.fan_interest) * 0.5).clamp(0.05, 1.0);
            }
        }
    }

    // ---- Free agency ----

    const ROSTER_MAX: usize = 15;
    /// Most outstanding offers the user may have at once.
    pub const FA_MAX_OFFERS: usize = 6;

    /// How many active offers the user currently has out.
    pub fn fa_offer_count(&self) -> usize {
        let Some(user) = self.user_team_id else { return 0 };
        self.free_agency.as_ref().map(|fa| fa.offers.iter().filter(|(_, o)| o.team == user).count()).unwrap_or(0)
    }

    /// A free agent's interest in the user's current offer (money vs market,
    /// nudged by team quality).
    pub fn fa_interest(&self, pid: PlayerId) -> Interest {
        let Some(user) = self.user_team_id else { return Interest::NoOffer };
        let Some(fa) = &self.free_agency else { return Interest::NoOffer };
        let Some(offer) = fa.user_offer(pid, user) else { return Interest::NoOffer };
        let Some(p) = self.players.iter().find(|p| p.id == pid) else { return Interest::NoOffer };

        let market = market_salary(p.overall()).max(MIN_SALARY) as f64;
        // Star weight 0..1: the better the player, the more he chases winning
        // over money (and the harder he is to simply outbid for).
        let star = ((p.overall() as f64 - 70.0) / 25.0).clamp(0.0, 1.0);
        let league_avg = self.teams.iter().map(|t| t.strength(&self.players)).sum::<f64>() / self.teams.len() as f64;
        let user_strength = self.teams.iter().find(|t| t.id == user).map(|t| t.strength(&self.players)).unwrap_or(league_avg);
        let score = Self::fa_appeal(offer.salary, offer.years, market, star, user_strength, league_avg, self.team_fa_bonus(user));

        if score >= 1.15 { Interest::Eager }
        else if score >= 0.95 { Interest::Interested }
        else if score >= 0.75 { Interest::Lukewarm }
        else { Interest::Unlikely }
    }

    /// How appealing an offer is to a free agent. Money has diminishing returns
    /// (capped at 1.6× market), and matters less to stars; contention (team
    /// strength vs league average) matters more to stars. `taste` is per-player
    /// randomness the sim adds so the richest bid isn't automatic.
    #[allow(clippy::too_many_arguments)]
    fn fa_appeal(salary: u32, years: u8, market: f64, star: f64, team_strength: f64, league_avg: f64, taste: f64) -> f64 {
        let ratio = (salary as f64 / market).min(1.6);
        let money_appeal = ratio * (1.0 - 0.35 * star);
        let str_norm = (team_strength - league_avg) / 20.0; // ~±0.5
        let contention = str_norm * (0.4 + 1.2 * star);
        let years_appeal = (years as f64 - 2.0) * 0.03;
        money_appeal + contention + years_appeal + taste
    }

    /// Open the offseason free-agency period. The pool is every unsigned player
    /// (expired contracts + undrafted prospects), best first.
    pub fn enter_free_agency(&mut self) {
        let mut pool: Vec<PlayerId> = self
            .players
            .iter()
            .filter(|p| p.team.is_none())
            .map(|p| p.id)
            .collect();
        pool.sort_by_key(|pid| {
            std::cmp::Reverse(self.players.iter().find(|p| p.id == *pid).map(|p| p.overall()).unwrap_or(0))
        });
        self.free_agency = Some(FreeAgency { round: 1, pool, offers: Vec::new(), log: Vec::new() });
        self.phase = Phase::FreeAgency;
    }

    fn roster_len(&self, tid: TeamId) -> usize {
        self.teams.iter().find(|t| t.id == tid).map(|t| t.roster.len()).unwrap_or(0)
    }

    /// The user makes (or replaces) an offer to a free agent. Returns false if
    /// it doesn't fit the roster or the cap.
    pub fn fa_user_offer(&mut self, pid: PlayerId, salary: u32, years: u8) -> bool {
        let Some(user) = self.user_team_id else { return false };
        let in_pool = self.free_agency.as_ref().map(|fa| fa.pool.contains(&pid)).unwrap_or(false);
        if !in_pool || self.roster_len(user) >= Self::ROSTER_MAX {
            return false;
        }
        let salary = salary.clamp(MIN_SALARY, 48_000);
        if (salary as i64) > self.team_cap_space(user) {
            return false;
        }
        // Enforce the offer cap (replacing an existing offer is always allowed).
        let replacing = self.free_agency.as_ref().map(|fa| fa.offers.iter().any(|(p, o)| *p == pid && o.team == user)).unwrap_or(false);
        if !replacing && self.fa_offer_count() >= Self::FA_MAX_OFFERS {
            return false;
        }
        let years = years.clamp(1, 5);
        if let Some(fa) = self.free_agency.as_mut() {
            fa.offers.retain(|(p, o)| !(*p == pid && o.team == user));
            fa.offers.push((pid, FaOffer { team: user, salary, years }));
        }
        true
    }

    /// Remove the user's offer to a player.
    pub fn fa_clear_user_offer(&mut self, pid: PlayerId) {
        let Some(user) = self.user_team_id else { return };
        if let Some(fa) = self.free_agency.as_mut() {
            fa.offers.retain(|(p, o)| !(*p == pid && o.team == user));
        }
    }

    /// Run one round of free agency: CPU teams make offers, then every free
    /// agent with an offer signs the most appealing one.
    pub fn fa_sim_round(&mut self) {
        let Some(user) = self.user_team_id else { return };
        let Some(fa) = self.free_agency.as_ref() else { return };
        let round = fa.round;

        // Precompute lookups so we can mutate players/teams afterward.
        let ovr: HashMap<PlayerId, u8> = self.players.iter().map(|p| (p.id, p.overall())).collect();
        let age: HashMap<PlayerId, u8> = self.players.iter().map(|p| (p.id, p.age)).collect();
        let pname: HashMap<PlayerId, String> = self.players.iter().map(|p| (p.id, p.name.clone())).collect();
        let abbrev: HashMap<TeamId, String> = self.teams.iter().map(|t| (t.id, t.abbrev.clone())).collect();
        let strength: HashMap<TeamId, f64> = self.teams.iter().map(|t| (t.id, t.strength(&self.players))).collect();
        let fa_bonus: HashMap<TeamId, f64> = self.teams.iter().map(|t| (t.id, self.team_fa_bonus(t.id))).collect();

        let mut room: HashMap<TeamId, i32> = self.teams.iter().map(|t| (t.id, Self::ROSTER_MAX as i32 - t.roster.len() as i32)).collect();
        let mut space: HashMap<TeamId, i64> = self.teams.iter().map(|t| (t.id, self.team_cap_space(t.id))).collect();

        let mut pool_sorted = fa.pool.clone();
        pool_sorted.sort_by_key(|pid| std::cmp::Reverse(*ovr.get(pid).unwrap_or(&0)));

        let mut offers: Vec<(PlayerId, FaOffer)> = fa.offers.clone();

        // CPU offers: each team chases the best couple of affordable FAs.
        let years_for = |a: u8| if a <= 27 { 4 } else if a <= 31 { 3 } else { 2 };
        for t in &self.teams {
            if t.id == user || room[&t.id] <= 0 {
                continue;
            }
            let mut sp = space[&t.id];
            let mut made = 0;
            for &pid in &pool_sorted {
                if made >= 2 {
                    break;
                }
                let mkt = market_salary(*ovr.get(&pid).unwrap_or(&50)) as i64;
                let already = offers.iter().any(|(p, o)| *p == pid && o.team == t.id);
                if !already && mkt <= sp {
                    offers.push((pid, FaOffer { team: t.id, salary: mkt as u32, years: years_for(*age.get(&pid).unwrap_or(&25)) }));
                    sp -= mkt;
                    made += 1;
                }
            }
        }

        // Resolve: best FAs first pick the most appealing valid offer. Appeal is
        // preference-based (money has diminishing returns; stars chase winning),
        // so the richest bid doesn't automatically win — especially for stars.
        let league_avg = strength.values().sum::<f64>() / strength.len().max(1) as f64;
        let mut rng = StdRng::seed_from_u64(self.seed ^ 0xFADE_u64 ^ round as u64);
        let mut signings: Vec<(PlayerId, FaOffer)> = Vec::new();
        for &pid in &pool_sorted {
            let p_ovr = *ovr.get(&pid).unwrap_or(&50);
            let market = market_salary(p_ovr).max(MIN_SALARY) as f64;
            let star = ((p_ovr as f64 - 70.0) / 25.0).clamp(0.0, 1.0);

            let mut best: Option<FaOffer> = None;
            let mut best_u = f64::MIN;
            for (p, o) in offers.iter().filter(|(p, _)| *p == pid) {
                let _ = p;
                if room[&o.team] <= 0 || (o.salary as i64) > space[&o.team] {
                    continue;
                }
                // Per-(player,team) taste; stars have a wider, noisier market.
                let taste = rng.gen_range(0.0..(0.15 + 0.5 * star)) + fa_bonus.get(&o.team).copied().unwrap_or(0.0);
                let u = Self::fa_appeal(o.salary, o.years, market, star, strength[&o.team], league_avg, taste);
                if u > best_u {
                    best_u = u;
                    best = Some(o.clone());
                }
            }
            if let Some(o) = best {
                // Stars may hold out early for a better fit rather than sign the
                // first decent offer — signing them takes patience.
                let holds_out = round <= 2 && star > 0.5 && rng.gen::<f64>() < 0.30 * star;
                // A merely tolerable offer might not get signed yet either.
                let too_cool = best_u < 0.7 && rng.gen::<f64>() < 0.5;
                if holds_out || too_cool {
                    continue;
                }
                *room.get_mut(&o.team).unwrap() -= 1;
                *space.get_mut(&o.team).unwrap() -= o.salary as i64;
                signings.push((pid, o));
            }
        }

        // Apply signings.
        let mut log = Vec::new();
        for (pid, o) in &signings {
            if let Some(p) = self.players.iter_mut().find(|p| p.id == *pid) {
                p.team = Some(o.team);
                p.contract = Contract { salary: o.salary, years: o.years };
            }
            if let Some(t) = self.teams.iter_mut().find(|t| t.id == o.team) {
                t.roster.push(*pid);
            }
            log.push(format!(
                "{} \u{2192} {} ({}, {}yr)",
                pname.get(pid).cloned().unwrap_or_default(),
                abbrev.get(&o.team).cloned().unwrap_or_default(),
                Contract { salary: o.salary, years: o.years }.salary_str(),
                o.years,
            ));
        }

        if let Some(fa) = self.free_agency.as_mut() {
            for (pid, _) in &signings {
                fa.pool.retain(|x| x != pid);
            }
            fa.offers.clear();
            fa.round += 1;
            fa.log = log;
        }
    }

    /// End free agency: fill out thin CPU rosters with the best remaining free
    /// agents (minimum deals), then tip off the new season.
    pub fn fa_finish(&mut self) {
        let user = self.user_team_id;
        let ovr: HashMap<PlayerId, u8> = self.players.iter().map(|p| (p.id, p.overall())).collect();
        // Every team needs a workable roster (>= 12).
        let team_ids: Vec<TeamId> = self.teams.iter().map(|t| t.id).collect();
        for tid in team_ids {
            if Some(tid) == user {
                continue; // the user manages their own roster
            }
            while self.roster_len(tid) < 12 {
                let mut pool: Vec<PlayerId> = self
                    .free_agency
                    .as_ref()
                    .map(|fa| fa.pool.clone())
                    .unwrap_or_default();
                pool.sort_by_key(|pid| std::cmp::Reverse(*ovr.get(pid).unwrap_or(&0)));
                let Some(pid) = pool.first().copied() else { break };
                let salary = market_salary(*ovr.get(&pid).unwrap_or(&50)).max(MIN_SALARY);
                if let Some(p) = self.players.iter_mut().find(|p| p.id == pid) {
                    p.team = Some(tid);
                    p.contract = Contract { salary, years: 2 };
                }
                if let Some(t) = self.teams.iter_mut().find(|t| t.id == tid) {
                    t.roster.push(pid);
                }
                if let Some(fa) = self.free_agency.as_mut() {
                    fa.pool.retain(|x| *x != pid);
                }
            }
        }
        self.start_new_season();
    }

    // ---- New season ----

    /// Roll into the next season: clear the draft, age players, reset records
    /// and stats, rebuild the schedule, and tip off. Rosters (including drafted
    /// rookies) and customizations are kept.
    pub fn start_new_season(&mut self) {
        self.season += 1;
        self.draft = None;
        self.free_agency = None;
        // Age and develop every player (young grow toward potential, vets
        // decline). Each team's coaching/training budget speeds up its players.
        let dev_factors: HashMap<TeamId, f64> = self.teams.iter().map(|t| (t.id, self.team_dev_factor(t.id))).collect();
        let mut dev_rng = StdRng::seed_from_u64(self.seed ^ 0xDE7_u64 ^ (self.season as u64));
        for p in &mut self.players {
            p.age = p.age.saturating_add(1);
            let factor = p.team.and_then(|t| dev_factors.get(&t).copied()).unwrap_or(1.0);
            develop_player(p, &mut dev_rng, factor);
        }
        for t in &mut self.teams {
            t.wins = 0;
            t.losses = 0;
        }
        self.season_stats = vec![SeasonStats::default(); self.players.len()];
        self.playoffs = None;
        // Extend the tradeable-pick window to cover the new season's horizon.
        self.ensure_pick_assets();
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
