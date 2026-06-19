//! Free-agency state: an offseason bidding period where the user and CPU teams
//! make contract offers and each free agent signs the most appealing one.

use crate::types::{PlayerId, TeamId};
use serde::{Deserialize, Serialize};

/// A contract offer to a free agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FaOffer {
    pub team: TeamId,
    /// Yearly salary, in thousands of dollars.
    pub salary: u32,
    pub years: u8,
}

/// Live free-agency state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FreeAgency {
    pub round: u32,
    /// Free agents still available, best first.
    pub pool: Vec<PlayerId>,
    /// Pending offers for the current round (user + CPU).
    pub offers: Vec<(PlayerId, FaOffer)>,
    /// Signings from the most recent resolved round (display text).
    pub log: Vec<String>,
}

impl FreeAgency {
    /// The user's current offer to a player, if any.
    pub fn user_offer(&self, pid: PlayerId, user: TeamId) -> Option<&FaOffer> {
        self.offers.iter().find(|(p, o)| *p == pid && o.team == user).map(|(_, o)| o)
    }
}
