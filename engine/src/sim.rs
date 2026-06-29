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

        // Minutes follow a realistic rotation template (best players first),
        // normalized to 240 team-minutes (5 men × 48). Using a template rather
        // than a power of overall keeps any one player from being handed an
        // impossible workload when a roster has a big talent gap.
        const TEMPLATE: [f64; 9] = [38.0, 36.0, 34.0, 32.0, 30.0, 22.0, 18.0, 16.0, 14.0];
        let raw: Vec<f64> = (0..roster.len()).map(|i| TEMPLATE[i.min(8)]).collect();
        let raw_sum: f64 = raw.iter().sum::<f64>().max(1.0);
        let minutes: Vec<f64> = raw
            .iter()
            .map(|m| (m * 240.0 / raw_sum).min(42.0))
            .collect();

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

/// A single play-by-play entry for the live "simcast" view. This is transient
/// presentation data (not persisted), so it can carry full box snapshots.
#[derive(Debug, Clone)]
pub struct PlayEvent {
    pub quarter: u8,
    pub clock: String,
    /// The team that had the ball.
    pub team_id: TeamId,
    pub text: String,
    /// Whether the possession scored (for highlighting).
    pub scored: bool,
    pub home_score: u32,
    pub away_score: u32,
    /// Box scores as of this event.
    pub home_box: Vec<PlayerLine>,
    pub away_box: Vec<PlayerLine>,
}

#[derive(Clone, Copy)]
enum PlayKind {
    Three,
    Layup,
    Dunk,
    FreeThrows,
    Miss,
    Turnover,
}

/// What a single possession produced (for play-by-play text).
struct PossOutcome {
    shooter: usize,
    points: u32,
    kind: PlayKind,
    and_one: bool,
    assist: Option<usize>,
    /// Defender index credited with a steal or block.
    defender: Option<usize>,
}

/// Simulate one game between two teams, with `home` enjoying home court.
pub fn simulate_game(home: &Team, away: &Team, players: &[Player], rng: &mut impl Rng) -> GameSim {
    run_game(home, away, players, rng, false).0
}

/// Simulate a game and also produce a play-by-play feed for the simcast.
pub fn simulate_game_pbp(home: &Team, away: &Team, players: &[Player], rng: &mut impl Rng) -> (GameSim, Vec<PlayEvent>) {
    run_game(home, away, players, rng, true)
}

fn run_game(home: &Team, away: &Team, players: &[Player], rng: &mut impl Rng, record: bool) -> (GameSim, Vec<PlayEvent>) {
    let mut h = Rotation::build(home, players);
    let mut a = Rotation::build(away, players);

    let possessions = rng.gen_range(98..=106);
    let total = possessions * 2;
    let secs_per = 2880.0 / total as f64; // 48:00 spread across all possessions
    let mut events = Vec::new();
    let mut played = 0u32;

    for _ in 0..possessions {
        let o = resolve_possession(&mut h, &mut a, HOME_EDGE, rng, 0);
        played += 1;
        if record {
            events.push(make_event(&h, &a, home.id, away.id, true, &o, played, secs_per));
        }
        let o2 = resolve_possession(&mut a, &mut h, 0.0, rng, 0);
        played += 1;
        if record {
            events.push(make_event(&h, &a, home.id, away.id, false, &o2, played, secs_per));
        }
    }

    let mut game = GameSim {
        home: finalize(home.id, h),
        away: finalize(away.id, a),
    };

    // Break ties: the better-shooting team gets an overtime bucket.
    if game.home.score == game.away.score {
        let hf: u32 = game.home.lines.iter().map(|l| l.fgm).sum();
        let af: u32 = game.away.lines.iter().map(|l| l.fgm).sum();
        let home_ot = hf >= af;
        if home_ot {
            bump_score(&mut game.home);
        } else {
            bump_score(&mut game.away);
        }
        if record {
            events.push(PlayEvent {
                quarter: 5,
                clock: "OT".into(),
                team_id: if home_ot { home.id } else { away.id },
                text: "Overtime winner!".into(),
                scored: true,
                home_score: game.home.score,
                away_score: game.away.score,
                home_box: game.home.lines.clone(),
                away_box: game.away.lines.clone(),
            });
        }
    }
    (game, events)
}

/// Build a play-by-play event from a possession outcome.
#[allow(clippy::too_many_arguments)]
fn make_event(h: &Rotation, a: &Rotation, home_id: TeamId, away_id: TeamId, off_is_home: bool, o: &PossOutcome, played: u32, secs_per: f64) -> PlayEvent {
    let (off, def, team_id) = if off_is_home { (h, a, home_id) } else { (a, h, away_id) };
    let name = |rot: &Rotation, i: usize| rot.players.get(i).map(|p| p.name.clone()).unwrap_or_default();
    let shooter = name(off, o.shooter);

    let mut text = match o.kind {
        PlayKind::Three => format!("{shooter} drains a three"),
        PlayKind::Dunk => format!("{shooter} throws it down"),
        PlayKind::Layup => format!("{shooter} finishes at the rim"),
        PlayKind::FreeThrows => format!("{shooter} hits {} at the line", o.points),
        PlayKind::Miss => match o.defender {
            Some(d) => format!("{shooter}'s shot rejected by {}", name(def, d)),
            None => format!("{shooter} misses"),
        },
        PlayKind::Turnover => match o.defender {
            Some(d) => format!("{} steals it from {shooter}", name(def, d)),
            None => format!("{shooter} turns it over"),
        },
    };
    if o.and_one {
        text.push_str(" — and the foul!");
    }
    if let Some(ai) = o.assist {
        text.push_str(&format!(" ({} assist)", name(off, ai)));
    }

    let home_score: u32 = h.lines.iter().map(|l| l.pts).sum();
    let away_score: u32 = a.lines.iter().map(|l| l.pts).sum();

    let elapsed = played as f64 * secs_per;
    let quarter = ((elapsed / 720.0).floor() as u8 + 1).min(4);
    let q_remaining = (720.0 - (elapsed - (quarter as f64 - 1.0) * 720.0)).max(0.0);
    let clock = format!("{}:{:02}", (q_remaining / 60.0) as u32, (q_remaining % 60.0) as u32);

    PlayEvent {
        quarter,
        clock,
        team_id,
        text,
        scored: o.points > 0,
        home_score,
        away_score,
        home_box: h.lines.clone(),
        away_box: a.lines.clone(),
    }
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
) -> PossOutcome {
    let shooter = weighted_pick(&off.usage, rng);
    let r = off.players[shooter].ratings.clone();
    let def_rating = def.team_defense;

    // --- Turnover: poor handle + tough defense force giveaways. ---
    let p_tov = (0.135 - (r.ball_handling as f64 - 50.0) * 0.0016 + (def_rating - 50.0) * 0.0016)
        .clamp(0.04, 0.30);
    if rng.gen::<f64>() < p_tov {
        off.lines[shooter].tov += 1;
        let mut defender = None;
        if rng.gen::<f64>() < 0.5 {
            let d = weighted_pick(&def.def_weight, rng);
            def.lines[d].stl += 1;
            defender = Some(d);
        }
        return PossOutcome { shooter, points: 0, kind: PlayKind::Turnover, and_one: false, assist: None, defender };
    }

    // --- Shot selection: three vs drive. ---
    let three_tendency = (0.12 + (r.three as f64 - 48.0) * 0.006).clamp(0.05, 0.62);
    let is_three = rng.gen::<f64>() < three_tendency;

    off.lines[shooter].fga += 1;
    let (made, points, assist_chance, kind) = if is_three {
        off.lines[shooter].tpa += 1;
        // Elite shooters land ~40% from deep; league ~35%.
        let p = (0.335 + (r.three as f64 - 55.0) * 0.004 - (def_rating - 50.0) * 0.0035 + home_edge)
            .clamp(0.18, 0.46);
        let made = rng.gen::<f64>() < p;
        if made {
            off.lines[shooter].tpm += 1;
        }
        (made, 3u32, 0.82, PlayKind::Three)
    } else {
        // Drive: ball handling + athleticism vs defense decides the look, and
        // whether it finishes as a dunk or a layup.
        let drive_adv = (r.ball_handling as f64 + r.athleticism as f64) / 2.0 - def_rating;
        let can_dunk = r.dunk >= 68 && r.athleticism >= 64 && drive_adv > -8.0;
        let dunk = can_dunk && rng.gen::<f64>() < 0.40;
        let finish = if dunk { r.dunk } else { r.layup } as f64;
        // Dunks convert high; layups are around the low-50s for good finishers.
        let base = if dunk { 0.68 } else { 0.52 };
        let p = (base + (finish - 60.0) * 0.0028 + drive_adv * 0.0028 - (def_rating - 50.0) * 0.0035 + home_edge)
            .clamp(0.20, 0.76);
        (rng.gen::<f64>() < p, 2u32, 0.55, if dunk { PlayKind::Dunk } else { PlayKind::Layup })
    };

    if made {
        off.lines[shooter].fgm += 1;
        off.lines[shooter].pts += points;
        let mut assist = None;
        if rng.gen::<f64>() < assist_chance {
            let passer = pick_other(&off.ast_weight, shooter, rng);
            off.lines[passer].ast += 1;
            assist = Some(passer);
        }
        let mut and_one = false;
        if points == 2 && rng.gen::<f64>() < 0.07 {
            and_one = free_throws(off, shooter, 1, rng) > 0;
        }
        return PossOutcome { shooter, points, kind, and_one, assist, defender: None };
    }

    // --- Miss: shooting foul, block, then the rebound battle. ---
    if points == 2 && rng.gen::<f64>() < 0.08 {
        let m = free_throws(off, shooter, 2, rng); // fouled in the act
        return PossOutcome { shooter, points: m, kind: PlayKind::FreeThrows, and_one: false, assist: None, defender: None };
    }
    let mut blocker = None;
    if points == 2 && rng.gen::<f64>() < 0.06 {
        let b = weighted_pick(&def.def_weight, rng);
        def.lines[b].blk += 1;
        blocker = Some(b);
    }

    // Offensive rebound? Second chance vs the defense's glass.
    let p_oreb = (0.23 * (off.team_reb / def.team_reb.max(1.0))).clamp(0.08, 0.40);
    if depth < 3 && rng.gen::<f64>() < p_oreb {
        let rebounder = weighted_pick(&off.reb_weight, rng);
        off.lines[rebounder].oreb += 1;
        return resolve_possession(off, def, home_edge, rng, depth + 1);
    }
    let rebounder = weighted_pick(&def.reb_weight, rng);
    def.lines[rebounder].dreb += 1;
    PossOutcome { shooter, points: 0, kind: PlayKind::Miss, and_one: false, assist: None, defender: blocker }
}

fn free_throws(off: &mut Rotation, shooter: usize, n: u32, rng: &mut impl Rng) -> u32 {
    let mut made = 0;
    for _ in 0..n {
        off.lines[shooter].fta += 1;
        if rng.gen::<f64>() < 0.77 {
            off.lines[shooter].ftm += 1;
            off.lines[shooter].pts += 1;
            made += 1;
        }
    }
    made
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
