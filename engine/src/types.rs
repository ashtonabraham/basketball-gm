//! Small shared value types used across the engine.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Strongly-typed ids so we never mix up a team id with a player id.
pub type TeamId = u32;
pub type PlayerId = u32;

/// An RGB color, stored as a hex string like `#1D428A` for easy use in the
/// web UI (the team builder lets the user edit these).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color(pub String);

impl Color {
    pub fn new(hex: &str) -> Self {
        Color(hex.to_string())
    }

    pub fn hex(&self) -> &str {
        &self.0
    }
}

/// The five basketball positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Position {
    PG,
    SG,
    SF,
    PF,
    C,
}

impl Position {
    pub const ALL: [Position; 5] = [
        Position::PG,
        Position::SG,
        Position::SF,
        Position::PF,
        Position::C,
    ];

    pub fn abbrev(&self) -> &'static str {
        match self {
            Position::PG => "PG",
            Position::SG => "SG",
            Position::SF => "SF",
            Position::PF => "PF",
            Position::C => "C",
        }
    }
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.abbrev())
    }
}

/// The two conferences. With 32 teams we run 16 per conference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Conference {
    East,
    West,
}

impl Conference {
    pub fn name(&self) -> &'static str {
        match self {
            Conference::East => "Eastern",
            Conference::West => "Western",
        }
    }
}
