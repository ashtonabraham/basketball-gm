//! The top-level league: all state plus the season state machine.
//!
//! This is the single object the UI holds. It is fully `serde`-serializable, so
//! the web layer can save/load it to the browser's localStorage as JSON.

use crate::names::{FIRST_NAMES, LAST_NAMES};
use crate::player::{Player, Ratings};
use crate::playoffs::{simulate_playoffs, Playoffs};
use crate::schedule::{generate_schedule, Game};
use crate::sim::sim_game;
use crate::standings::playoff_seeds;
use crate::team::Team;
use crate::teams_data::PRESETS;
use crate::types::{Color, Conference, PlayerId, Position, TeamId};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};

/// Where we are in the yearly cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    /// Picking and customizing your team.
    TeamSelect,
    RegularSeason,
    Playoffs,
    /// Season over; recap is available.
    Offseason,
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

        // Each attribute is the team talent plus per-player and per-position
        // variation, clamped to 0–100.
        let base = talent + rng.gen_range(-12.0..14.0);
        let attr = |modifier: f64, rng: &mut dyn rand::RngCore| -> u8 {
            (base + modifier + rng.gen_range(-8.0..8.0)).clamp(25.0, 99.0) as u8
        };
        let (inside_m, outside_m, play_m, reb_m, def_m, ath_m) = match pos {
            Position::PG => (-6.0, 6.0, 10.0, -8.0, 0.0, 4.0),
            Position::SG => (-2.0, 8.0, 2.0, -6.0, 0.0, 4.0),
            Position::SF => (2.0, 2.0, 0.0, 0.0, 2.0, 2.0),
            Position::PF => (6.0, -4.0, -4.0, 6.0, 2.0, 0.0),
            Position::C => (8.0, -8.0, -6.0, 10.0, 4.0, -2.0),
        };
        let ratings = Ratings {
            inside: attr(inside_m, rng),
            outside: attr(outside_m, rng),
            playmaking: attr(play_m, rng),
            rebounding: attr(reb_m, rng),
            defense: attr(def_m, rng),
            athleticism: attr(ath_m, rng),
        };

        Player {
            id,
            name,
            age: rng.gen_range(19..=38),
            position: pos,
            ratings,
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

    fn team_strength(&self, team_id: TeamId) -> f64 {
        self.teams
            .iter()
            .find(|t| t.id == team_id)
            .map(|t| t.strength(&self.players))
            .unwrap_or(50.0)
    }

    /// Simulate every game on the given day, updating team records.
    fn sim_specific_day(&mut self, day: u32) {
        let mut rng = StdRng::seed_from_u64(self.seed.wrapping_mul(1_000_003).wrapping_add(day as u64));
        // Collect strengths first to avoid borrow conflicts.
        let indices: Vec<usize> = self
            .schedule
            .iter()
            .enumerate()
            .filter(|(_, g)| g.day == day && !g.is_played())
            .map(|(i, _)| i)
            .collect();
        for i in indices {
            let (home, away) = (self.schedule[i].home, self.schedule[i].away);
            let hs = self.team_strength(home);
            let as_ = self.team_strength(away);
            let res = sim_game(hs, as_, &mut rng);
            // Update records.
            let (hw, aw) = if res.home_won() { (true, false) } else { (false, true) };
            if let Some(t) = self.teams.iter_mut().find(|t| t.id == home) {
                if hw { t.wins += 1 } else { t.losses += 1 }
            }
            if let Some(t) = self.teams.iter_mut().find(|t| t.id == away) {
                if aw { t.wins += 1 } else { t.losses += 1 }
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

    /// Seed the bracket from final standings and play it out.
    pub fn start_playoffs(&mut self) {
        assert!(self.regular_season_complete(), "regular season not finished");
        let east = playoff_seeds(&self.teams, Conference::East);
        let west = playoff_seeds(&self.teams, Conference::West);

        // Snapshot strengths so the closure doesn't borrow self mutably.
        let strengths: Vec<(TeamId, f64)> = self
            .teams
            .iter()
            .map(|t| (t.id, t.strength(&self.players)))
            .collect();
        let strength = |id: TeamId| {
            strengths
                .iter()
                .find(|(tid, _)| *tid == id)
                .map(|(_, s)| *s)
                .unwrap_or(50.0)
        };

        let mut rng = StdRng::seed_from_u64(self.seed ^ 0x71A04FF_u64);
        let po = simulate_playoffs(&east, &west, strength, &mut rng);
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

    /// Roll into the next season: age players, reset records, rebuild the
    /// schedule, and tip off a new regular season. Rosters and customizations
    /// are kept. (Draft/free agency come later.)
    pub fn start_new_season(&mut self) {
        self.season += 1;
        for p in &mut self.players {
            p.age = p.age.saturating_add(1);
        }
        for t in &mut self.teams {
            t.wins = 0;
            t.losses = 0;
        }
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
