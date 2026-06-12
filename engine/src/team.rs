//! A team in the league.

use crate::player::Player;
use crate::types::{Color, Conference, PlayerId, TeamId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    pub id: TeamId,
    /// Fixed in the team builder (one of the 32 preset locations).
    pub location: String,
    /// Editable nickname (defaults to the preset, e.g. "Celtics").
    pub name: String,
    pub abbrev: String,
    pub primary: Color,
    pub secondary: Color,
    pub conference: Conference,
    /// Ids of players on the roster.
    pub roster: Vec<PlayerId>,
    /// Regular-season record.
    pub wins: u32,
    pub losses: u32,
}

impl Team {
    /// Full display name, e.g. "Boston Celtics".
    pub fn full_name(&self) -> String {
        format!("{} {}", self.location, self.name)
    }

    pub fn games_played(&self) -> u32 {
        self.wins + self.losses
    }

    pub fn win_pct(&self) -> f64 {
        let g = self.games_played();
        if g == 0 {
            0.0
        } else {
            self.wins as f64 / g as f64
        }
    }

    /// Team strength = average overall of the best 8 players (rotation depth).
    /// Used by the rating-based game simulator.
    pub fn strength(&self, players: &[Player]) -> f64 {
        let mut ovrs: Vec<u8> = self
            .roster
            .iter()
            .filter_map(|pid| players.iter().find(|p| p.id == *pid))
            .map(|p| p.overall())
            .collect();
        if ovrs.is_empty() {
            return 50.0;
        }
        ovrs.sort_unstable_by(|a, b| b.cmp(a));
        let n = ovrs.len().min(8);
        ovrs[..n].iter().map(|o| *o as f64).sum::<f64>() / n as f64
    }
}
