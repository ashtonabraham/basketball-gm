//! Playoffs: best-of-7 bracket. Top 8 seeds per conference, conference
//! quarterfinals → semifinals → conference finals → Finals.
//!
//! Game simulation is injected as a closure so the league can run the full
//! possession sim (and accumulate playoff stats) while this module just manages
//! the bracket.

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

pub const ROUND_NAMES: [&str; 4] = [
    "First Round",
    "Conference Semifinals",
    "Conference Finals",
    "Finals",
];

/// 2-2-1-1-1: which game numbers the higher seed hosts.
fn high_seed_hosts(game_no: usize) -> bool {
    matches!(game_no, 1 | 2 | 5 | 7)
}

/// Play one best-of-7 series. `sim` takes (home, away) and returns the result.
/// `?Sized` so it accepts both `impl FnMut` and a `&mut dyn FnMut`.
fn play_series<F: FnMut(TeamId, TeamId) -> GameResult + ?Sized>(
    high: TeamId,
    low: TeamId,
    sim: &mut F,
) -> Series {
    let mut s = Series { high, low, high_wins: 0, low_wins: 0, games: Vec::new() };
    let mut game_no = 1;
    while s.high_wins < 4 && s.low_wins < 4 {
        let (home, away) = if high_seed_hosts(game_no) { (high, low) } else { (low, high) };
        let res = sim(home, away);
        let high_won = if home == high { res.home_won() } else { !res.home_won() };
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

fn first_round_pairs(seeds: &[TeamId]) -> Vec<(TeamId, TeamId)> {
    assert_eq!(seeds.len(), 8);
    vec![
        (seeds[0], seeds[7]),
        (seeds[3], seeds[4]),
        (seeds[1], seeds[6]),
        (seeds[2], seeds[5]),
    ]
}

/// Simulate the entire postseason. `seed_of` maps a team to its conference seed
/// (1 = best); the lower seed hosts in later rounds. `sim` runs a single game.
pub fn simulate_playoffs(
    east: &[TeamId],
    west: &[TeamId],
    seed_of: impl Fn(TeamId) -> u32,
    mut sim: impl FnMut(TeamId, TeamId) -> GameResult,
) -> Playoffs {
    let mut rounds: Vec<Vec<Series>> = Vec::new();

    // First round: 4 series per conference (East then West).
    let mut r0 = Vec::new();
    for (h, l) in first_round_pairs(east).into_iter().chain(first_round_pairs(west)) {
        r0.push(play_series(h, l, &mut sim));
    }
    rounds.push(r0);

    // Higher seed (lower number) hosts.
    let advance = |a: TeamId, b: TeamId, sim: &mut dyn FnMut(TeamId, TeamId) -> GameResult| {
        let (high, low) = if seed_of(a) <= seed_of(b) { (a, b) } else { (b, a) };
        play_series(high, low, sim)
    };

    // Conference semifinals.
    let w = |i: usize| rounds[0][i].winner().unwrap();
    let semis = vec![
        advance(w(0), w(1), &mut sim),
        advance(w(2), w(3), &mut sim),
        advance(w(4), w(5), &mut sim),
        advance(w(6), w(7), &mut sim),
    ];
    rounds.push(semis);

    // Conference finals.
    let s = |i: usize| rounds[1][i].winner().unwrap();
    let conf_finals = vec![
        advance(s(0), s(1), &mut sim),
        advance(s(2), s(3), &mut sim),
    ];
    rounds.push(conf_finals);

    // Finals.
    let f = |i: usize| rounds[2][i].winner().unwrap();
    let finals = advance(f(0), f(1), &mut sim);
    let champion = finals.winner();
    rounds.push(vec![finals]);

    Playoffs { rounds, champion }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn playoffs_produce_a_champion() {
        let east: Vec<TeamId> = (0..8).collect();
        let west: Vec<TeamId> = (8..16).collect();
        let seed_of = |id: TeamId| (id % 8) + 1;
        let mut rng = rand::rngs::StdRng::seed_from_u64(5);
        // Simple stub sim: home usually wins.
        let sim = |_home: TeamId, _away: TeamId| {
            use rand::Rng;
            let home_won = rng.gen::<f64>() < 0.6;
            GameResult {
                home_score: if home_won { 110 } else { 100 },
                away_score: if home_won { 100 } else { 110 },
            }
        };
        let po = simulate_playoffs(&east, &west, seed_of, sim);

        assert_eq!(po.rounds.len(), 4);
        assert_eq!(po.rounds[0].len(), 8);
        assert_eq!(po.rounds[1].len(), 4);
        assert_eq!(po.rounds[2].len(), 2);
        assert_eq!(po.rounds[3].len(), 1);
        assert!(po.champion.is_some());
        for round in &po.rounds {
            for s in round {
                assert!(s.winner().is_some());
                assert!((4..=7).contains(&s.games.len()));
            }
        }
    }
}
