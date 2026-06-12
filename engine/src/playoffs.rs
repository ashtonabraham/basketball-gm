//! Playoffs: best-of-7 bracket. Top 8 seeds per conference, conference
//! quarterfinals → semifinals → conference finals → Finals.

use crate::schedule::GameResult;
use crate::sim::sim_game;
use crate::types::TeamId;
use rand::Rng;
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
    pub fn winner(&self) -> Option<TeamId> {
        if self.high_wins >= 4 {
            Some(self.high)
        } else if self.low_wins >= 4 {
            Some(self.low)
        } else {
            None
        }
    }
}

/// Full bracket. `rounds[0]` is the first round (8 series), then 4, then 2,
/// then the single Finals series in `rounds[3]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playoffs {
    pub rounds: Vec<Vec<Series>>,
    pub champion: Option<TeamId>,
}

/// Names for each round, for display.
pub const ROUND_NAMES: [&str; 4] = [
    "First Round",
    "Conference Semifinals",
    "Conference Finals",
    "Finals",
];

/// 2-2-1-1-1 home layout: which game numbers are hosted by the higher seed.
fn high_seed_hosts(game_no: usize) -> bool {
    matches!(game_no, 1 | 2 | 5 | 7)
}

/// Play out one best-of-7 series given a strength lookup.
fn play_series(
    high: TeamId,
    low: TeamId,
    strength: &impl Fn(TeamId) -> f64,
    rng: &mut impl Rng,
) -> Series {
    let mut s = Series { high, low, high_wins: 0, low_wins: 0, games: Vec::new() };
    let mut game_no = 1;
    while s.high_wins < 4 && s.low_wins < 4 {
        let high_home = high_seed_hosts(game_no);
        let (home, away) = if high_home { (high, low) } else { (low, high) };
        let res = sim_game(strength(home), strength(away), rng);
        let high_won = if high_home { res.home_won() } else { !res.home_won() };
        if high_won {
            s.high_wins += 1;
        } else {
            s.low_wins += 1;
        }
        s.games.push(res);
        game_no += 1;
    }
    s
}

/// Pair seeds 1v8, 2v7, 3v6, 4v5 within a conference's 8-team field.
fn first_round_pairs(seeds: &[TeamId]) -> Vec<(TeamId, TeamId)> {
    assert_eq!(seeds.len(), 8);
    vec![
        (seeds[0], seeds[7]),
        (seeds[3], seeds[4]),
        (seeds[1], seeds[6]),
        (seeds[2], seeds[5]),
    ]
}

/// Simulate the entire postseason. `east`/`west` are the 8 conference seeds in
/// seed order (1..8). `strength` maps a team id to its current strength, and is
/// also used to decide home court between two series winners (higher strength
/// hosts).
pub fn simulate_playoffs(
    east: &[TeamId],
    west: &[TeamId],
    strength: impl Fn(TeamId) -> f64,
    rng: &mut impl Rng,
) -> Playoffs {
    let mut rounds: Vec<Vec<Series>> = Vec::new();

    // --- First round: 4 series per conference (East then West) ---
    let mut series_round: Vec<Series> = Vec::new();
    for (h, l) in first_round_pairs(east) {
        series_round.push(play_series(h, l, &strength, rng));
    }
    for (h, l) in first_round_pairs(west) {
        series_round.push(play_series(h, l, &strength, rng));
    }
    rounds.push(series_round);

    // Helper to seed the next series: higher strength gets home court.
    let next_series = |a: TeamId, b: TeamId, rng: &mut dyn rand::RngCore| -> Series {
        let (high, low) = if strength(a) >= strength(b) { (a, b) } else { (b, a) };
        // play_series needs impl Rng; wrap the dyn ref.
        play_series(high, low, &strength, &mut RngRef(rng))
    };

    // --- Conference semifinals: winners of (0,1) and (2,3) per conference ---
    let r0 = &rounds[0];
    let semis = vec![
        next_series(r0[0].winner().unwrap(), r0[1].winner().unwrap(), rng), // East
        next_series(r0[2].winner().unwrap(), r0[3].winner().unwrap(), rng), // East
        next_series(r0[4].winner().unwrap(), r0[5].winner().unwrap(), rng), // West
        next_series(r0[6].winner().unwrap(), r0[7].winner().unwrap(), rng), // West
    ];
    rounds.push(semis);

    // --- Conference finals ---
    let r1 = &rounds[1];
    let conf_finals = vec![
        next_series(r1[0].winner().unwrap(), r1[1].winner().unwrap(), rng), // East champ
        next_series(r1[2].winner().unwrap(), r1[3].winner().unwrap(), rng), // West champ
    ];
    rounds.push(conf_finals);

    // --- Finals ---
    let r2 = &rounds[2];
    let finals = next_series(r2[0].winner().unwrap(), r2[1].winner().unwrap(), rng);
    let champion = finals.winner();
    rounds.push(vec![finals]);

    Playoffs { rounds, champion }
}

/// Tiny adapter so we can pass a `&mut dyn RngCore` where `impl Rng` is wanted.
struct RngRef<'a>(&'a mut dyn rand::RngCore);
impl rand::RngCore for RngRef<'_> {
    fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }
    fn next_u64(&mut self) -> u64 {
        self.0.next_u64()
    }
    fn fill_bytes(&mut self, dest: &mut [u8]) {
        self.0.fill_bytes(dest)
    }
    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        self.0.try_fill_bytes(dest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn playoffs_produce_a_champion() {
        let east: Vec<TeamId> = (0..8).collect();
        let west: Vec<TeamId> = (8..16).collect();
        // Strength roughly decreasing by seed.
        let strength = |id: TeamId| 70.0 - (id % 8) as f64 * 1.5;
        let mut rng = rand::rngs::StdRng::seed_from_u64(5);
        let po = simulate_playoffs(&east, &west, strength, &mut rng);

        assert_eq!(po.rounds.len(), 4);
        assert_eq!(po.rounds[0].len(), 8);
        assert_eq!(po.rounds[1].len(), 4);
        assert_eq!(po.rounds[2].len(), 2);
        assert_eq!(po.rounds[3].len(), 1);
        assert!(po.champion.is_some());
        // Every series must have a winner and 4..=7 games.
        for round in &po.rounds {
            for s in round {
                assert!(s.winner().is_some());
                assert!((4..=7).contains(&s.games.len()));
            }
        }
    }
}
