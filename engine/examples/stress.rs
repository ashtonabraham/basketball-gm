// Stress the full season cycle for many years, checking invariants each loop so
// a broken mechanic surfaces as a panic rather than a silently corrupt game.
use engine::{market_salary, League, Phase};

fn check_invariants(l: &League, label: &str) {
    // Every player is on at most one roster, and rosters are sane sizes.
    let mut seen = std::collections::HashSet::new();
    for t in &l.teams {
        assert!(t.roster.len() <= 15, "{label}: {} over 15", t.abbrev);
        for pid in &t.roster {
            assert!(seen.insert(*pid), "{label}: player {pid} on two rosters");
            let p = l.players.iter().find(|p| p.id == *pid).expect("roster player exists");
            assert_eq!(p.team, Some(t.id), "{label}: {} team mismatch", p.name);
        }
    }
    // Pick assets never reference a draft that already happened.
    for pk in &l.pick_assets {
        assert!(pk.season >= l.season, "{label}: stale pick {} < {}", pk.season, l.season);
    }
}

fn manage_user_fa(l: &mut League) {
    // Keep the user roster populated (mimics a GM filling out the bench).
    for _ in 0..8 {
        let pool = l.free_agency.as_ref().map(|f| f.pool.clone()).unwrap_or_default();
        for pid in pool.iter().take(6) {
            let ovr = l.players.iter().find(|p| p.id == *pid).map(|p| p.overall()).unwrap_or(60);
            l.fa_user_offer(*pid, market_salary(ovr), 3);
        }
        l.fa_sim_round();
    }
    l.fa_finish();
}

fn main() {
    let mut l = League::new(2024);
    l.select_team(0);
    let seasons = 15u32;
    let mut fa_landed = 0u32;
    let mut fa_targets = 0u32;
    let mut mid_landed = 0u32;
    let mut mid_targets = 0u32;
    let mut strength_ranks: Vec<usize> = Vec::new();

    for yr in 1..=seasons {
        l.sim_to_end_of_season();
        for t in &l.teams {
            assert_eq!(t.games_played(), 82, "yr {yr}: {} played {}", t.abbrev, t.games_played());
        }
        // Where does the user team rank in strength (1 = best of 32)?
        let mut strengths: Vec<(u32, f64)> = l.teams.iter().map(|t| (t.id, t.strength(&l.players))).collect();
        strengths.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        strength_ranks.push(strengths.iter().position(|(id, _)| *id == 0).unwrap() + 1);
        l.start_playoffs();
        l.playoff_sim_all();
        assert!(l.playoffs_complete(), "yr {yr}: playoffs didn't finish");
        l.finish_season();
        assert_eq!(l.phase, Phase::Offseason);
        check_invariants(&l, &format!("yr {yr} offseason"));

        l.enter_draft();
        l.draft_sim_all();
        assert!(l.draft_complete());
        l.enter_free_agency();

        // FA difficulty probe: the user (team 0) chases the 3 best free agents at
        // a strong overpay; count how many they actually land. Also chase a
        // mid-tier free agent (pool rank ~10) which should be easy to sign.
        let top: Vec<u32> = l.free_agency.as_ref().unwrap().pool.iter().take(3).copied().collect();
        let mid: Option<u32> = l.free_agency.as_ref().unwrap().pool.get(10).copied();
        for pid in &top {
            let ovr = l.players.iter().find(|p| p.id == *pid).map(|p| p.overall()).unwrap_or(60);
            l.fa_user_offer(*pid, (market_salary(ovr) as f64 * 1.4) as u32, 4);
            fa_targets += 1;
        }
        if let Some(pid) = mid {
            let ovr = l.players.iter().find(|p| p.id == pid).map(|p| p.overall()).unwrap_or(60);
            l.fa_user_offer(pid, (market_salary(ovr) as f64 * 1.2) as u32, 3);
            mid_targets += 1;
        }
        manage_user_fa(&mut l);
        for pid in &top {
            if l.players.iter().find(|p| p.id == *pid).and_then(|p| p.team) == Some(0) {
                fa_landed += 1;
            }
        }
        if let Some(pid) = mid {
            if l.players.iter().find(|p| p.id == pid).and_then(|p| p.team) == Some(0) {
                mid_landed += 1;
            }
        }

        l.start_new_season();
        for t in &l.teams {
            assert_eq!(t.games_played(), 0, "yr {yr}: records not reset");
        }
        check_invariants(&l, &format!("yr {yr} newseason"));
    }

    let avg_rank = strength_ranks.iter().sum::<usize>() as f64 / strength_ranks.len() as f64;
    println!("Completed {seasons} full seasons with no invariant violations.");
    println!("User team avg strength rank: {avg_rank:.1} of 32 (lower = better).");
    println!("FA top-3 targets landed: {fa_landed}/{fa_targets}   mid-tier landed: {mid_landed}/{mid_targets}");
    println!("Final season {}, players {}, careers tracked {}", l.season, l.players.len(), l.careers.len());
}
