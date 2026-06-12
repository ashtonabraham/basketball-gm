//! The annual draft: a lottery-seeded, 2-round draft of generated prospects.
//!
//! Order is decided in [`crate::league`] (it needs the standings); this module
//! holds the draft state and the pick board. Picks are made one at a time —
//! the user picks for their team, the CPU auto-picks best-available for the
//! rest, and the league exposes "sim to my pick" / "sim entire draft" helpers.

use crate::types::{PlayerId, TeamId};
use serde::{Deserialize, Serialize};

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
