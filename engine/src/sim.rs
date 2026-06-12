//! Game simulation.
//!
//! v1 is a rating-based model: win probability comes from the two teams'
//! strength difference plus a home-court edge, and a believable final score is
//! generated around a league-average pace. This lives behind a small function
//! so it can later be swapped for a possession-by-possession engine without
//! touching the season/playoff code that calls it.

use crate::schedule::GameResult;
use rand::Rng;

/// Home teams win roughly 60% of evenly-matched games in real basketball;
/// this is the rating bump (in overall points) we give the home side.
const HOME_EDGE: f64 = 3.5;

/// Controls how much a rating gap swings win probability. Larger = ratings
/// matter less (more upsets).
const RATING_SPREAD: f64 = 9.0;

/// League-average points per game; individual games vary around this.
const BASE_POINTS: f64 = 112.0;

/// Simulate one game from each side's team strength (0–100ish).
/// Returns the final score. `home`/`away` are the strength values.
pub fn sim_game(home_strength: f64, away_strength: f64, rng: &mut impl Rng) -> GameResult {
    let diff = (home_strength + HOME_EDGE) - away_strength;
    // Logistic win probability for the home team.
    let p_home = 1.0 / (1.0 + (-diff / RATING_SPREAD).exp());

    // Generate two scores. The stronger team scores a bit more on average;
    // a margin is layered on so the winner matches the probability roll.
    let home_won = rng.gen::<f64>() < p_home;

    // Base scoring nudged by strength relative to league average (50).
    let home_pts_base = BASE_POINTS + (home_strength - 50.0) * 0.35 + HOME_EDGE;
    let away_pts_base = BASE_POINTS + (away_strength - 50.0) * 0.35;

    // Random game-to-game noise.
    let noise = |rng: &mut dyn rand::RngCore| -> f64 {
        // Roughly normal-ish: average a few uniforms.
        let mut s = 0.0;
        for _ in 0..4 {
            s += rng.gen_range(-1.0..1.0);
        }
        s / 4.0 * 14.0
    };

    let mut home = (home_pts_base + noise(rng)).round() as i32;
    let mut away = (away_pts_base + noise(rng)).round() as i32;

    // Force the result to match who "should" win, and avoid ties.
    if home_won && home <= away {
        home = away + rng.gen_range(1..=8);
    } else if !home_won && away <= home {
        away = home + rng.gen_range(1..=8);
    }
    if home == away {
        // Overtime nudge.
        if home_won {
            home += 2;
        } else {
            away += 2;
        }
    }

    GameResult {
        home_score: home.max(70) as u32,
        away_score: away.max(70) as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;

    #[test]
    fn never_ties_and_scores_reasonable() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(3);
        for _ in 0..1000 {
            let r = sim_game(60.0, 55.0, &mut rng);
            assert_ne!(r.home_score, r.away_score);
            assert!(r.home_score >= 70 && r.home_score < 200);
            assert!(r.away_score >= 70 && r.away_score < 200);
        }
    }

    #[test]
    fn stronger_team_wins_more_often() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(99);
        let mut strong_wins = 0;
        for _ in 0..2000 {
            let r = sim_game(70.0, 50.0, &mut rng);
            if r.home_won() {
                strong_wins += 1;
            }
        }
        // Big favorite at home should win well over 70% of the time.
        assert!(strong_wins > 1400, "strong team only won {strong_wins}/2000");
    }
}
