//! Quick sanity check: sim a full season and print league leaders + a sample
//! box-score-derived stat line. Run with `cargo run -p engine --example season_stats`.

use engine::League;

fn main() {
    let mut league = League::new(2024);
    league.select_team(0);
    league.sim_to_end_of_season();

    // Build (player, stats) pairs.
    let mut rows: Vec<(&engine::Player, &engine::SeasonStats)> = league
        .players
        .iter()
        .map(|p| (p, &league.season_stats[p.id as usize]))
        .filter(|(_, s)| s.gp > 0)
        .collect();

    println!("=== Top 10 scorers ===");
    rows.sort_by(|a, b| b.1.ppg().partial_cmp(&a.1.ppg()).unwrap());
    for (p, s) in rows.iter().take(10) {
        let team = league.teams.iter().find(|t| Some(t.id) == p.team).unwrap();
        println!(
            "{:<22} {:<3} {} | {:>4.1} pts  {:>4.1} reb  {:>4.1} ast  {:.3} FG%  {:.3} 3P%  ovr {}",
            p.name, p.position.abbrev(), team.abbrev,
            s.ppg(), s.rpg(), s.apg(), s.fg_pct(), s.tp_pct(), p.overall(),
        );
    }

    // League scoring average per team game.
    let total_pts: u32 = league.season_stats.iter().map(|s| s.pts).sum();
    let team_games: u32 = league.teams.iter().map(|t| t.games_played()).sum();
    println!("\nLeague avg team points/game: {:.1}", total_pts as f64 / team_games as f64);
}
