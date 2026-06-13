//! The annual draft: a lottery-seeded, 2-round draft of generated prospects.
//!
//! Order is decided in [`crate::league`] (it needs the standings); this module
//! holds the draft state and the pick board. Picks are made one at a time —
//! the user picks for their team, the CPU auto-picks best-available for the
//! rest, and the league exposes "sim to my pick" / "sim entire draft" helpers.

use crate::types::{PlayerId, TeamId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Your scouting read on a prospect: a noisy estimate of his true potential and
/// how uncertain that estimate still is. Scouting refines both.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoutEntry {
    /// Current best guess at the prospect's peak overall.
    pub estimate: f64,
    /// Remaining uncertainty (std-dev-ish); shrinks as you scout.
    pub uncertainty: f64,
}

impl ScoutEntry {
    /// The displayed letter grade, derived from the current estimate.
    pub fn grade(&self) -> &'static str {
        grade_for(self.estimate)
    }

    /// A 0–3 confidence level for UI (more scouting = higher confidence).
    pub fn confidence(&self) -> u8 {
        match self.uncertainty {
            u if u <= 3.0 => 3,
            u if u <= 6.0 => 2,
            u if u <= 10.0 => 1,
            _ => 0,
        }
    }
}

/// Map a peak-overall value to a scouting letter grade.
pub fn grade_for(value: f64) -> &'static str {
    match value.round() as i32 {
        90.. => "A+",
        85..=89 => "A",
        80..=84 => "A-",
        76..=79 => "B+",
        72..=75 => "B",
        68..=71 => "B-",
        64..=67 => "C+",
        60..=63 => "C",
        55..=59 => "C-",
        50..=54 => "D",
        _ => "F",
    }
}

/// A single slot in the draft.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DraftPick {
    pub round: u8,
    /// Overall pick number, 1..=64.
    pub overall: u8,
    pub team_id: TeamId,
    /// Set once the pick is made.
    pub player_id: Option<PlayerId>,
}

/// Live draft state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Draft {
    /// All 64 picks, in order.
    pub picks: Vec<DraftPick>,
    /// Ids of prospects still available.
    pub prospects: Vec<PlayerId>,
    /// Index into `picks` of the team currently on the clock.
    pub on_clock: usize,
    /// Your scouting read on each prospect, keyed by player id.
    pub scouting: HashMap<PlayerId, ScoutEntry>,
    /// Remaining scouting actions you can spend this draft.
    pub scout_points: u32,
}

impl Draft {
    pub fn is_complete(&self) -> bool {
        self.on_clock >= self.picks.len()
    }

    /// The pick currently on the clock, if the draft is still running.
    pub fn current(&self) -> Option<&DraftPick> {
        self.picks.get(self.on_clock)
    }

    /// The team currently on the clock.
    pub fn team_on_clock(&self) -> Option<TeamId> {
        self.current().map(|p| p.team_id)
    }
}
