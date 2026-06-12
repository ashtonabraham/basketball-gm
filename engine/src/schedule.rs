//! Regular-season schedule generation.
//!
//! 32 teams, 82 games each. We use the classic round-robin "circle method"
//! to build rounds where every team plays exactly once. Layering rounds gives
//! a balanced 82-game season with no team playing twice on the same day:
//!
//!   * Pass A: full single round-robin (31 days) — play everyone once at home/away.
//!   * Pass B: the same 31 rounds with home/away swapped (31 days).
//!   * Pass C: the first 20 rounds again (20 days) to reach 82.
//!
//! 31 + 31 + 20 = 82 games per team.

use crate::types::TeamId;
use rand::seq::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};

/// The outcome of a played game.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct GameResult {
    pub home_score: u32,
    pub away_score: u32,
}

impl GameResult {
    pub fn home_won(&self) -> bool {
        self.home_score > self.away_score
    }
}

/// A single scheduled game on a given day (0-based).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Game {
    pub day: u32,
    pub home: TeamId,
    pub away: TeamId,
    pub result: Option<GameResult>,
}

impl Game {
    pub fn is_played(&self) -> bool {
        self.result.is_some()
    }
}

/// Generate 31 round-robin rounds for the given team ids using the circle
/// method. Each round is a list of (home, away) pairs covering every team once.
fn round_robin_rounds(team_ids: &[TeamId]) -> Vec<Vec<(TeamId, TeamId)>> {
    let n = team_ids.len();
    debug_assert!(n % 2 == 0, "circle method needs an even team count");
    let mut ids: Vec<TeamId> = team_ids.to_vec();
    let mut rounds = Vec::with_capacity(n - 1);
    for _ in 0..(n - 1) {
        let mut round = Vec::with_capacity(n / 2);
        for i in 0..n / 2 {
            round.push((ids[i], ids[n - 1 - i]));
        }
        rounds.push(round);
        // Rotate everyone except the first fixed element.
        ids[1..].rotate_right(1);
    }
    rounds
}

/// Build the full 82-game schedule for the given teams.
pub fn generate_schedule(team_ids: &[TeamId], rng: &mut impl Rng) -> Vec<Game> {
    assert_eq!(team_ids.len(), 32, "schedule expects exactly 32 teams");

    // Shuffle so each save's schedule differs.
    let mut ids = team_ids.to_vec();
    ids.shuffle(rng);

    let rounds = round_robin_rounds(&ids);
    let mut games = Vec::with_capacity(32 * 82 / 2);
    let mut day = 0u32;

    let push_round = |games: &mut Vec<Game>, day: &mut u32, round: &[(TeamId, TeamId)], swap: bool| {
        for &(a, b) in round {
            let (home, away) = if swap { (b, a) } else { (a, b) };
            games.push(Game { day: *day, home, away, result: None });
        }
        *day += 1;
    };

    // Pass A: everyone once.
    for r in &rounds {
        push_round(&mut games, &mut day, r, false);
    }
    // Pass B: everyone once, home/away swapped.
    for r in &rounds {
        push_round(&mut games, &mut day, r, true);
    }
    // Pass C: first 20 rounds again to reach 82.
    for r in rounds.iter().take(20) {
        push_round(&mut games, &mut day, r, false);
    }

    games
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn every_team_plays_exactly_82() {
        let ids: Vec<TeamId> = (0..32).collect();
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        let games = generate_schedule(&ids, &mut rng);

        let mut counts = vec![0u32; 32];
        for g in &games {
            counts[g.home as usize] += 1;
            counts[g.away as usize] += 1;
        }
        for (t, c) in counts.iter().enumerate() {
            assert_eq!(*c, 82, "team {t} has {c} games");
        }
        assert_eq!(games.len(), 32 * 82 / 2);
    }

    #[test]
    fn no_team_plays_twice_in_a_day() {
        let ids: Vec<TeamId> = (0..32).collect();
        let mut rng = rand::rngs::StdRng::seed_from_u64(1);
        let games = generate_schedule(&ids, &mut rng);

        use std::collections::HashSet;
        let mut by_day: std::collections::HashMap<u32, HashSet<TeamId>> = Default::default();
        for g in &games {
            let set = by_day.entry(g.day).or_default();
            assert!(set.insert(g.home), "team played twice on day {}", g.day);
            assert!(set.insert(g.away), "team played twice on day {}", g.day);
        }
    }
}
