//! Diagnostic: sim multiple seasons (with development) and print the scoring
//! leaders + a usage check. Run with `cargo run -p engine --example season_stats`.

use engine::League;

fn dump(league: &League, season: u32) {
    let mut rows: Vec<(&engine::Player, &engine::SeasonStats)> = league
        .players
        .iter()
        .map(|p| (p, &league.season_stats[p.id as usize]))
        .filter(|(_, s)| s.gp > 0)
        .collect();
    rows.sort_by(|a, b| b.1.ppg().partial_cmp(&a.1.ppg()).unwrap());

    let over40 = rows.iter().filter(|(_, s)| s.ppg() >= 40.0).count();
    let over30 = rows.iter().filter(|(_, s)| s.ppg() >= 30.0).count();
    println!("\n===== Season {season} — top scorers ({over30} over 30ppg, {over40} over 40ppg) =====");
    for (p, s) in rows.iter().take(8) {
        let fga = s.fga as f64 / s.gp as f64;
        println!(
            "{:<22} ovr {:>2} | {:>4.1} pts on {:>4.1} FGA  {:.3} FG%  {:>4.1} mpg",
            p.name, p.overall(), s.ppg(), fga, s.fg_pct(), s.mpg(),
        );
    }
    let total_pts: u32 = league.season_stats.iter().map(|s| s.pts).sum();
    let team_games: u32 = league.teams.iter().map(|t| t.games_played()).sum();
    println!("league avg team ppg: {:.1}", total_pts as f64 / team_games as f64);
}

fn main() {
    let mut league = League::new(2024);
    league.select_team(0);

    for season in 1..=3 {
        league.sim_to_end_of_season();
        dump(&league, season);
        league.start_playoffs();
        league.playoff_sim_all();
        league.finish_season();
        league.enter_draft();
        league.draft_sim_all();
        league.start_new_season();
    }
}
