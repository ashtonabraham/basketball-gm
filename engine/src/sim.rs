//! Possession-based game simulation.
//!
//! Each game is played out one possession at a time, alternating between the
//! two teams. On every possession the offense picks a player (weighted by
//! minutes × usage), who either turns it over or attempts a shot. Which shot,
//! and whether it falls, is driven by that player's individual attributes
//! versus the defense:
//!
//!   * `ball_handling` + `athleticism` help a player beat his man on a drive,
//!     raising his finish chance and lowering turnovers.
//!   * `three` governs how often he shoots from deep and how often it falls.
//!   * `layup` / `dunk` govern interior finishing (dunk for high-flyers).
//!   * `passing` decides who collects assists.
//!   * `rebounding` decides who grabs misses (and second-chance points).
//!   * `defense` + `athleticism` suppress opponents and earn steals/blocks.
//!
//! The output is a full per-player box score, so attributes flow all the way
//! through to individual stats and the final score.

use crate::player::Player;
use crate::team::Team;
use crate::types::{PlayerId, TeamId};
use rand::Rng;
use serde::{Deserialize, Serialize};

/// One player's line in a single game.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlayerLine {
    pub player_id: PlayerId,
    pub min: u32,
    pub pts: u32,
    pub fgm: u32,
    pub fga: u32,
    pub tpm: u32,
    pub tpa: u32,
    pub ftm: u32,
    pub fta: u32,
    pub oreb: u32,
    pub dreb: u32,
    pub ast: u32,
    pub stl: u32,
    pub blk: u32,
    pub tov: u32,
}

/// One team's side of a box score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamBox {
    pub team_id: TeamId,
    pub score: u32,
    pub lines: Vec<PlayerLine>,
}

/// A fully simulated game.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameSim {
    pub home: TeamBox,
    pub away: TeamBox,
}

impl GameSim {
    pub fn home_won(&self) -> bool {
        self.home.score > self.away.score
    }
}

/// A team's active rotation plus the per-player weights the sim samples from.
struct Rotation<'a> {
    players: Vec<&'a Player>,
    lines: Vec<PlayerLine>,
    usage: Vec<f64>,      // share of offensive possessions
    reb_weight: Vec<f64>, // share of rebounds
    ast_weight: Vec<f64>, // share of assists
    def_weight: Vec<f64>, // share of steals/blocks
    team_defense: f64,    // minutes-weighted defense rating
    team_reb: f64,        // minutes-weighted rebounding rating
}

impl<'a> Rotation<'a> {
    fn build(team: &Team, players: &'a [Player]) -> Rotation<'a> {
        let mut roster: Vec<&Player> = team
            .roster
            .iter()
            .filter_map(|pid| players.iter().find(|p| p.id == *pid))
            .collect();
        roster.sort_by(|a, b| b.overall().cmp(&a.overall()));
        roster.truncate(9); // top 9 play

        // Minutes weighted by overall (stars play more), normalized to 240
        // team-minutes (5 men × 48).
        let pow: Vec<f64> = roster.iter().map(|p| (p.overall() as f64).powf(2.2)).collect();
        let pow_sum: f64 = pow.iter().sum::<f64>().max(1.0);
        let minutes: Vec<f64> = pow.iter().map(|p| 240.0 * p / pow_sum).collect();

        let usage = roster.iter().zip(&minutes).map(|(p, m)| m * p.ratings.scoring()).collect();
        let reb_weight = roster.iter().zip(&minutes).map(|(p, m)| m * p.ratings.rebounding as f64).collect();
        let ast_weight = roster.iter().zip(&minutes).map(|(p, m)| m * p.ratings.passing as f64).collect();
        let def_weight = roster
            .iter()
            .zip(&minutes)
            .map(|(p, m)| m * (p.ratings.defense as f64 + p.ratings.athleticism as f64))
            .collect();

        let team_defense = roster.iter().zip(&minutes).map(|(p, m)| m * p.ratings.defense as f64).sum::<f64>() / 240.0;
        let team_reb = roster.iter().zip(&minutes).map(|(p, m)| m * p.ratings.rebounding as f64).sum::<f64>() / 240.0;

        let lines = roster
            .iter()
            .zip(&minutes)
            .map(|(p, m)| PlayerLine { player_id: p.id, min: m.round() as u32, ..Default::default() })
            .collect();

        Rotation { players: roster, lines, usage, reb_weight, ast_weight, def_weight, team_defense, team_reb }
    }
}

/// Pick an index from a weight slice.
fn weighted_pick(weights: &[f64], rng: &mut impl Rng) -> usize {
    let total: f64 = weights.iter().sum();
    if total <= 0.0 {
        return 0;
    }
    let mut r = rng.gen::<f64>() * total;
    for (i, w) in weights.iter().enumerate() {
        r -= w;
        if r <= 0.0 {
            return i;
        }
    }
    weights.len() - 1
}

/// Pick an index by weight, excluding `exclude`.
fn pick_other(weights: &[f64], exclude: usize, rng: &mut impl Rng) -> usize {
    let mut w = weights.to_vec();
    if exclude < w.len() {
        w[exclude] = 0.0;
    }
    weighted_pick(&w, rng)
}

const HOME_EDGE: f64 = 0.02; // make-probability bump for the home side

/// Simulate one game between two teams, with `home` enjoying home court.
pub fn simulate_game(home: &Team, away: &Team, players: &[Player], rng: &mut impl Rng) -> GameSim {
    let mut h = Rotation::build(home, players);
    let mut a = Rotation::build(away, players);

    let possessions = rng.gen_range(95..=103);
    for _ in 0..possessions {
        resolve_possession(&mut h, &mut a, HOME_EDGE, rng, 0);
        resolve_possession(&mut a, &mut h, 0.0, rng, 0);
    }

    let mut game = GameSim {
        home: finalize(home.id, h),
        away: finalize(away.id, a),
    };

    // Break ties: the better-shooting team gets an overtime bucket.
    if game.home.score == game.away.score {
        let hf: u32 = game.home.lines.iter().map(|l| l.fgm).sum();
        let af: u32 = game.away.lines.iter().map(|l| l.fgm).sum();
        if hf >= af {
            bump_score(&mut game.home);
        } else {
            bump_score(&mut game.away);
        }
    }
    game
}

fn finalize(team_id: TeamId, rot: Rotation) -> TeamBox {
    let score = rot.lines.iter().map(|l| l.pts).sum();
    TeamBox { team_id, score, lines: rot.lines }
}

fn bump_score(tb: &mut TeamBox) {
    if let Some(line) = tb.lines.iter_mut().max_by_key(|l| l.fga) {
        line.pts += 2;
        line.fgm += 1;
        line.fga += 1;
    }
    tb.score = tb.lines.iter().map(|l| l.pts).sum();
}

/// Play out a single possession. Recurses (up to a few times) on offensive
/// rebounds for second-chance points. `def` is mutable so steals, blocks, and
/// defensive rebounds are credited to the defending players.
fn resolve_possession(
    off: &mut Rotation,
    def: &mut Rotation,
    home_edge: f64,
    rng: &mut impl Rng,
    depth: u32,
) {
    let shooter = weighted_pick(&off.usage, rng);
    let r = off.players[shooter].ratings.clone();
    let def_rating = def.team_defense;

    // --- Turnover: poor handle + tough defense force giveaways. ---
    let p_tov = (0.135 - (r.ball_handling as f64 - 50.0) * 0.0016 + (def_rating - 50.0) * 0.0016)
        .clamp(0.04, 0.30);
    if rng.gen::<f64>() < p_tov {
        off.lines[shooter].tov += 1;
        if rng.gen::<f64>() < 0.5 {
            let d = weighted_pick(&def.def_weight, rng);
            def.lines[d].stl += 1;
        }
        return;
    }

    // --- Shot selection: three vs drive. ---
    let three_tendency = (0.12 + (r.three as f64 - 48.0) * 0.006).clamp(0.05, 0.62);
    let is_three = rng.gen::<f64>() < three_tendency;

    off.lines[shooter].fga += 1;
    let (made, points, assist_chance) = if is_three {
        off.lines[shooter].tpa += 1;
        let p = (0.34 + (r.three as f64 - 50.0) * 0.005 - (def_rating - 50.0) * 0.0035 + home_edge)
            .clamp(0.20, 0.52);
        let made = rng.gen::<f64>() < p;
        if made {
            off.lines[shooter].tpm += 1;
        }
        (made, 3u32, 0.82)
    } else {
        // Drive: ball handling + athleticism vs defense decides the look, and
        // whether it finishes as a dunk or a layup.
        let drive_adv = (r.ball_handling as f64 + r.athleticism as f64) / 2.0 - def_rating;
        let can_dunk = r.dunk >= 68 && r.athleticism >= 64 && drive_adv > -8.0;
        let dunk = can_dunk && rng.gen::<f64>() < 0.40;
        let finish = if dunk { r.dunk } else { r.layup } as f64;
        let base = if dunk { 0.74 } else { 0.55 };
        let p = (base + (finish - 60.0) * 0.004 + drive_adv * 0.0035 - (def_rating - 50.0) * 0.003 + home_edge)
            .clamp(0.22, 0.86);
        (rng.gen::<f64>() < p, 2u32, 0.55)
    };

    if made {
        off.lines[shooter].fgm += 1;
        off.lines[shooter].pts += points;
        if rng.gen::<f64>() < assist_chance {
            let passer = pick_other(&off.ast_weight, shooter, rng);
            off.lines[passer].ast += 1;
        }
        if points == 2 && rng.gen::<f64>() < 0.07 {
            free_throws(off, shooter, 1, rng); // and-one
        }
        return;
    }

    // --- Miss: shooting foul, block, then the rebound battle. ---
    if points == 2 && rng.gen::<f64>() < 0.08 {
        free_throws(off, shooter, 2, rng); // fouled in the act
        return;
    }
    if points == 2 && rng.gen::<f64>() < 0.06 {
        let blocker = weighted_pick(&def.def_weight, rng);
        def.lines[blocker].blk += 1;
    }

    // Offensive rebound? Second chance vs the defense's glass.
    let p_oreb = (0.23 * (off.team_reb / def.team_reb.max(1.0))).clamp(0.08, 0.40);
    if depth < 3 && rng.gen::<f64>() < p_oreb {
        let rebounder = weighted_pick(&off.reb_weight, rng);
        off.lines[rebounder].oreb += 1;
        resolve_possession(off, def, home_edge, rng, depth + 1);
    } else {
        let rebounder = weighted_pick(&def.reb_weight, rng);
        def.lines[rebounder].dreb += 1;
    }
}

fn free_throws(off: &mut Rotation, shooter: usize, n: u32, rng: &mut impl Rng) {
    for _ in 0..n {
        off.lines[shooter].fta += 1;
        if rng.gen::<f64>() < 0.77 {
            off.lines[shooter].ftm += 1;
            off.lines[shooter].pts += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::League;
    use rand::SeedableRng;

    #[test]
    fn box_score_is_internally_consistent() {
        let league = League::new(11);
        let mut rng = rand::rngs::StdRng::seed_from_u64(1);
        let g = simulate_game(&league.teams[0], &league.teams[1], &league.players, &mut rng);
        for side in [&g.home, &g.away] {
            let pts: u32 = side.lines.iter().map(|l| l.pts).sum();
            assert_eq!(pts, side.score);
            for l in &side.lines {
                assert!(l.fgm <= l.fga);
                assert!(l.tpm <= l.tpa);
                assert!(l.ftm <= l.fta);
                assert_eq!(l.pts, 2 * (l.fgm - l.tpm) + 3 * l.tpm + l.ftm);
            }
        }
        assert_ne!(g.home.score, g.away.score, "no ties");
    }

    #[test]
    fn scores_are_realistic() {
        let league = League::new(11);
        let mut rng = rand::rngs::StdRng::seed_from_u64(2);
        let (mut total, n) = (0u32, 200);
        for _ in 0..n {
            let g = simulate_game(&league.teams[0], &league.teams[1], &league.players, &mut rng);
            total += g.home.score + g.away.score;
        }
        let avg_team = total as f64 / (2 * n) as f64;
        assert!(avg_team > 90.0 && avg_team < 125.0, "avg team score {avg_team}");
    }
}
