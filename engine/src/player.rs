//! Players and their ratings.
//!
//! Ratings are stored as individual 0–100 attributes (BBGM-style) so the
//! simulator can get deeper over time. For now an `overall` is derived from
//! them and drives the rating-based game sim.

use crate::types::{PlayerId, Position, TeamId};
use serde::{Deserialize, Serialize};

/// A player's skill attributes, each 0–100.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ratings {
    pub inside: u8,      // finishing at the rim
    pub outside: u8,     // jump shooting / three-point
    pub playmaking: u8,  // passing / handling
    pub rebounding: u8,
    pub defense: u8,
    pub athleticism: u8, // speed, strength, jumping
}

impl Ratings {
    /// A single 0–100 number summarizing the player. This is what the v1
    /// rating-based simulator uses; the weighting can be tuned later.
    pub fn overall(&self) -> u8 {
        let sum = self.inside as u32
            + self.outside as u32
            + self.playmaking as u32
            + self.rebounding as u32
            + self.defense as u32
            + self.athleticism as u32;
        (sum / 6) as u8
    }
}

/// A single player.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
    pub age: u8,
    pub position: Position,
    pub ratings: Ratings,
    /// `None` for free agents (not used yet, but the model supports it).
    pub team: Option<TeamId>,
}

impl Player {
    pub fn overall(&self) -> u8 {
        self.ratings.overall()
    }
}
