//! Players, their ratings, and accumulated season statistics.
//!
//! Ratings are individual 0–100 attributes. They are intentionally granular so
//! the possession simulator can let each one genuinely affect outcomes (e.g.
//! `ball_handling` helps a player beat his defender, producing more makes and
//! fewer turnovers). When adding new game systems, prefer wiring them to these
//! attributes so everything carries real weight.

use crate::types::{PlayerId, Position, TeamId};
use serde::{Deserialize, Serialize};

/// A player's skill attributes, each 0–100.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ratings {
    /// Finishing at the rim off drives / floaters.
    pub layup: u8,
    /// Finishing above the rim; rewards athleticism on drives.
    pub dunk: u8,
    /// Three-point shooting.
    pub three: u8,
    /// Passing — drives assist creation and assist accuracy.
    pub passing: u8,
    /// Ball handling — beating defenders off the dribble; reduces turnovers.
    pub ball_handling: u8,
    pub rebounding: u8,
    pub defense: u8,
    /// Speed / strength / leaping; aids drives, finishing, steals, blocks.
    pub athleticism: u8,
}

impl Ratings {
    /// A single 0–100 summary used for sorting and roster strength. Weighted
    /// toward the attributes that move the needle most in the sim.
    pub fn overall(&self) -> u8 {
        let s = self.layup as f64 * 1.0
            + self.dunk as f64 * 0.7
            + self.three as f64 * 1.1
            + self.passing as f64 * 0.9
            + self.ball_handling as f64 * 0.9
            + self.rebounding as f64 * 0.8
            + self.defense as f64 * 1.2
            + self.athleticism as f64 * 0.8;
        const W: f64 = 1.0 + 0.7 + 1.1 + 0.9 + 0.9 + 0.8 + 1.2 + 0.8;
        (s / W).round() as u8
    }

    /// Composite scoring punch — used to weight how often a player is used on
    /// offense (his "usage").
    pub fn scoring(&self) -> f64 {
        (self.layup as f64 + self.dunk as f64 + self.three as f64) / 3.0
            + self.ball_handling as f64 * 0.25
    }
}

/// A player's contract. Salary is in thousands of dollars per year (so 28000
/// means $28.0M). `years` counts seasons remaining, including the upcoming one;
/// 0 means the player is a free agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    pub salary: u32,
    pub years: u8,
}

impl Contract {
    pub fn free_agent() -> Self {
        Contract { salary: 0, years: 0 }
    }
    pub fn is_expired(&self) -> bool {
        self.years == 0
    }
    /// Salary formatted like "$28.0M".
    pub fn salary_str(&self) -> String {
        format!("${:.1}M", self.salary as f64 / 1000.0)
    }
}

/// A player.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
    pub age: u8,
    pub position: Position,
    pub ratings: Ratings,
    /// True peak overall this player can reach (the development ceiling). This
    /// is the exact value shown on rosters; for undrafted prospects it is
    /// hidden behind a scouted letter grade.
    pub potential: u8,
    /// `None` for free agents / undrafted prospects.
    pub team: Option<TeamId>,
    /// The season in which this player was drafted (used to identify rookies).
    /// `None` for the initial league population.
    pub draft_season: Option<u32>,
    /// Current contract (salary + years remaining).
    pub contract: Contract,
}

impl Player {
    pub fn overall(&self) -> u8 {
        self.ratings.overall()
    }

    /// Whether the player still has untapped upside.
    pub fn has_upside(&self) -> bool {
        self.potential > self.overall()
    }
}

/// Accumulated regular-season totals for one player. Stored separately from
/// `Player` (indexed by player id) so the simulator can read rosters
/// immutably while writing stats.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SeasonStats {
    pub gp: u32,
    pub min: u32,
    pub pts: u32,
    pub fgm: u32,
    pub fga: u32,
    pub tpm: u32,
    pub tpa: u32,
    pub ftm: u32,
    pub fta: u32,
    pub oreb: u32,
    pub dreb: u32,
    pub ast: u32,
    pub stl: u32,
    pub blk: u32,
    pub tov: u32,
}

impl SeasonStats {
    pub fn reb(&self) -> u32 {
        self.oreb + self.dreb
    }

    fn per_game(&self, total: u32) -> f64 {
        if self.gp == 0 {
            0.0
        } else {
            total as f64 / self.gp as f64
        }
    }

    pub fn ppg(&self) -> f64 {
        self.per_game(self.pts)
    }
    pub fn rpg(&self) -> f64 {
        self.per_game(self.reb())
    }
    pub fn apg(&self) -> f64 {
        self.per_game(self.ast)
    }
    pub fn mpg(&self) -> f64 {
        self.per_game(self.min)
    }

    pub fn fg_pct(&self) -> f64 {
        if self.fga == 0 { 0.0 } else { self.fgm as f64 / self.fga as f64 }
    }
    pub fn tp_pct(&self) -> f64 {
        if self.tpa == 0 { 0.0 } else { self.tpm as f64 / self.tpa as f64 }
    }
}
