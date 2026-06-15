//! Playoff data structures and bracket helpers.
//!
//! The bracket is advanced incrementally by [`crate::league`] one game-day at a
//! time (every live series plays its next game), so the postseason can be
//! watched game-by-game just like the regular season.

use crate::schedule::GameResult;
use crate::types::TeamId;
use serde::{Deserialize, Serialize};

/// A best-of-7 series between a higher seed (home-court advantage) and a lower
/// seed. First to 4 wins advances.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Series {
    pub high: TeamId,
    pub low: TeamId,
    pub high_wins: u8,
    pub low_wins: u8,
    pub games: Vec<GameResult>,
}

impl Series {
    pub fn new(high: TeamId, low: TeamId) -> Self {
        Series { high, low, high_wins: 0, low_wins: 0, games: Vec::new() }
    }

    pub fn winner(&self) -> Option<TeamId> {
        if self.high_wins >= 4 {
            Some(self.high)
        } else if self.low_wins >= 4 {
            Some(self.low)
        } else {
            None
        }
    }

    pub fn is_decided(&self) -> bool {
        self.winner().is_some()
    }

    pub fn games_played(&self) -> usize {
        self.games.len()
    }

    pub fn has_team(&self, id: TeamId) -> bool {
        self.high == id || self.low == id
    }
}

/// Full bracket. `rounds[0]` is the first round (8 series), filled in as it is
/// played; later rounds are appended as earlier ones finish.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playoffs {
    pub rounds: Vec<Vec<Series>>,
    pub champion: Option<TeamId>,
    /// Set when the champion is crowned: the best performer in the Finals.
    pub finals_mvp: Option<crate::types::PlayerId>,
}

pub const ROUND_NAMES: [&str; 4] = [
    "First Round",
    "Conference Semifinals",
    "Conference Finals",
    "Finals",
];

/// 2-2-1-1-1: which game numbers the higher seed hosts.
pub(crate) fn high_seed_hosts(game_no: usize) -> bool {
    matches!(game_no, 1 | 2 | 5 | 7)
}

/// Pair seeds 1v8, 2v7, 3v6, 4v5 within a conference's 8-team field.
pub(crate) fn first_round_pairs(seeds: &[TeamId]) -> Vec<(TeamId, TeamId)> {
    assert_eq!(seeds.len(), 8);
    vec![
        (seeds[0], seeds[7]),
        (seeds[3], seeds[4]),
        (seeds[1], seeds[6]),
        (seeds[2], seeds[5]),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn series_needs_four_wins() {
        let mut s = Series::new(1, 8);
        assert!(!s.is_decided());
        s.high_wins = 4;
        assert_eq!(s.winner(), Some(1));
    }

    #[test]
    fn home_layout_is_2_2_1_1_1() {
        let hosts: Vec<bool> = (1..=7).map(high_seed_hosts).collect();
        assert_eq!(hosts, vec![true, true, false, false, true, false, true]);
    }
}
