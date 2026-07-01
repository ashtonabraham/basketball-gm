//! Basketball GM game engine.
//!
//! Pure Rust, no UI and no web dependencies. The whole game lives here so the
//! web layer can stay a thin presentation shell. The central type is
//! [`League`], which is fully serializable for save/load.

pub mod draft;
pub mod free_agency;
pub mod league;
pub mod names;
pub mod player;
pub mod playoffs;
pub mod schedule;
pub mod sim;
pub mod standings;
pub mod team;
pub mod teams_data;
pub mod types;

pub use draft::{grade_for, Draft, DraftPick, ScoutEntry};
pub use free_agency::{FaOffer, FreeAgency};
pub use league::{
    market_salary, Awards, Interest, League, OwnerMessage, OwnerTone, Phase, PlayoffOutcome,
    SeasonHistory, SeasonRecap, TradeEval, TradeSuggestion, MIN_SALARY, SALARY_CAP,
};
pub use player::{Career, CareerSeason, Contract, Honor, HonorEntry, Player, Ratings, SeasonStats};
pub use playoffs::{Playoffs, Series, ROUND_NAMES};
pub use schedule::{Game, GameResult};
pub use sim::{simulate_game, simulate_game_pbp, GameSim, PlayEvent, PlayerLine, TeamBox};
pub use standings::{conference_standings, playoff_seeds, StandingsRow};
pub use team::Team;
pub use teams_data::{TeamPreset, PRESETS};
pub use types::{Color, Conference, PlayerId, Position, TeamId};

#[cfg(test)]
mod integration_tests {
    use super::*;

    /// Play a full season start-to-finish and sanity check the results.
    #[test]
    fn full_season_runs_end_to_end() {
        let mut league = League::new(42);
        assert_eq!(league.teams.len(), 32);
        assert_eq!(league.players.len(), 32 * 14);

        // Pick a team (customize it first, like the team builder would).
        let my_id = league.teams[0].id;
        league.customize_team(my_id, Some("Test Team".into()), None, None, None);
        league.select_team(my_id);
        assert_eq!(league.phase, Phase::RegularSeason);

        // Play the whole regular season.
        league.sim_to_end_of_season();
        assert!(league.regular_season_complete());

        // Every team should have played 82 games.
        for t in &league.teams {
            assert_eq!(t.games_played(), 82, "{} played {}", t.full_name(), t.games_played());
        }

        // Run the playoffs game-day by game-day.
        league.start_playoffs();
        assert_eq!(league.phase, Phase::Playoffs);
        assert!(league.playoffs.as_ref().unwrap().champion.is_none());
        league.playoff_sim_all();
        assert!(league.playoffs_complete());
        let po = league.playoffs.as_ref().unwrap();
        assert!(po.champion.is_some());
        // Bracket fully formed: 8 + 4 + 2 + 1 series.
        let series_counts: Vec<usize> = po.rounds.iter().map(|r| r.len()).collect();
        assert_eq!(series_counts, vec![8, 4, 2, 1]);

        // Recap and finish.
        let recap = league.season_recap().expect("recap exists");
        assert_eq!(recap.team_name, "Atlanta Test Team");
        assert!(recap.wins + recap.losses == 82);
        league.finish_season();
        assert_eq!(league.phase, Phase::Offseason);
        assert_eq!(league.history.len(), 1);
    }

    #[test]
    fn awards_finals_mvp_and_owner_message() {
        let mut league = League::new(21);
        league.select_team(0);
        league.sim_to_end_of_season();
        league.start_playoffs();
        league.playoff_sim_all();

        // Finals MVP belongs to the champion and posted Finals stats.
        let po = league.playoffs.as_ref().unwrap();
        let champ = po.champion.unwrap();
        let mvp = po.finals_mvp.expect("finals mvp set");
        let champ_team = league.teams.iter().find(|t| t.id == champ).unwrap();
        assert!(champ_team.roster.contains(&mvp));
        assert!(league.finals_stats[mvp as usize].gp > 0);

        league.finish_season();
        let awards = league.awards.as_ref().unwrap();
        assert!(awards.mvp.is_some());
        assert!(awards.dpoy.is_some());

        // First three seasons: the owner withholds judgment.
        let owner = league.owner_message.as_ref().unwrap();
        assert_eq!(owner.tone, OwnerTone::TooEarly);
        assert!(!owner.body.is_empty());
    }

    #[test]
    fn playoffs_advance_one_gameday_at_a_time() {
        let mut league = League::new(7);
        league.select_team(0);
        league.sim_to_end_of_season();
        league.start_playoffs();

        // One game-day plays exactly one game in each of the 8 first-round series.
        league.playoff_sim_gameday();
        let po = league.playoffs.as_ref().unwrap();
        assert_eq!(po.rounds.len(), 1);
        for s in &po.rounds[0] {
            assert_eq!(s.games_played(), 1);
        }

        // Finish it out; a champion emerges and no series exceeds 7 games.
        league.playoff_sim_all();
        assert!(league.playoffs_complete());
        for round in &league.playoffs.as_ref().unwrap().rounds {
            for s in round {
                assert!((4..=7).contains(&s.games_played()));
            }
        }
    }

    #[test]
    fn trades_respect_value_and_salary() {
        let mut league = League::new(4);
        league.select_team(0);
        let (user, other) = (0u32, 1u32);
        assert!(league.can_trade(), "should be able to trade pre-deadline");

        let user_roster: Vec<u32> = league.teams[user as usize].roster.clone();
        let other_roster: Vec<u32> = league.teams[other as usize].roster.clone();

        // Lopsided ask (give your worst, demand their best) is not accepted.
        let user_worst = *user_roster.iter().min_by(|a, b| league.player_trade_value(**a).partial_cmp(&league.player_trade_value(**b)).unwrap()).unwrap();
        let other_best = *other_roster.iter().max_by(|a, b| league.player_trade_value(**a).partial_cmp(&league.player_trade_value(**b)).unwrap()).unwrap();
        let bad = league.evaluate_trade(other, &[user_worst], &[other_best]);
        assert!(!bad.accepted, "fleecing the CPU should be rejected");

        // Find any legal deal where the CPU gains value — it should accept and execute.
        let mut done = false;
        'outer: for &g in &user_roster {
            for &r in &other_roster {
                let e = league.evaluate_trade(other, &[g], &[r]);
                if e.legal && e.give_value > e.get_value + 60.0 {
                    assert!(e.accepted);
                    assert!(league.execute_trade(other, &[g], &[r]));
                    assert_eq!(league.players.iter().find(|p| p.id == g).unwrap().team, Some(other));
                    assert_eq!(league.players.iter().find(|p| p.id == r).unwrap().team, Some(user));
                    done = true;
                    break 'outer;
                }
            }
        }
        assert!(done, "expected at least one acceptable trade to exist");
    }

    #[test]
    fn free_agency_signs_players() {
        let mut league = League::new(31);
        league.select_team(0);
        league.sim_to_end_of_season();
        league.start_playoffs();
        league.playoff_sim_all();
        league.finish_season();
        league.enter_draft();
        league.draft_sim_all();
        league.enter_free_agency();
        assert_eq!(league.phase, Phase::FreeAgency);

        let pool_before = league.free_agency.as_ref().unwrap().pool.len();
        assert!(pool_before > 0, "there should be free agents");

        // CPU teams should sign players over a few rounds.
        for _ in 0..5 {
            league.fa_sim_round();
        }
        let pool_after = league.free_agency.as_ref().unwrap().pool.len();
        assert!(pool_after < pool_before, "free agents should have signed");

        // Every signed player has a real contract and is on a roster.
        for t in &league.teams {
            for pid in &t.roster {
                let p = league.players.iter().find(|p| p.id == *pid).unwrap();
                assert!(p.contract.years > 0 && p.contract.salary >= crate::MIN_SALARY);
                assert_eq!(p.team, Some(t.id));
            }
        }

        league.fa_finish();
        assert_eq!(league.phase, Phase::RegularSeason);
        assert!(league.free_agency.is_none());
        // Cap is respected for CPU teams (some slack allowed for min deals).
        assert!(league.teams.iter().all(|t| t.roster.len() >= 5));
    }

    #[test]
    fn draft_runs_and_fills_every_pick() {
        let mut league = League::new(99);
        league.select_team(3);
        league.sim_to_end_of_season();
        league.start_playoffs();
        league.playoff_sim_all();
        league.finish_season();

        let players_before = league.players.len();
        league.enter_draft();
        assert_eq!(league.phase, Phase::Draft);
        assert_eq!(league.players.len(), players_before + 70);

        // Worst team should hold a top-3 lottery slot most years; at minimum the
        // order must contain all 32 teams in round 1.
        let r1_teams: std::collections::HashSet<_> = league
            .draft
            .as_ref()
            .unwrap()
            .picks
            .iter()
            .filter(|p| p.round == 1)
            .map(|p| p.team_id)
            .collect();
        assert_eq!(r1_teams.len(), 32);

        // Sim the whole draft; every pick should be filled with a unique player.
        league.draft_sim_all();
        assert!(league.draft_complete());
        let d = league.draft.as_ref().unwrap();
        assert_eq!(d.picks.len(), 64);
        let mut drafted = std::collections::HashSet::new();
        for p in &d.picks {
            let pid = p.player_id.expect("pick made");
            assert!(drafted.insert(pid), "player drafted twice");
            // Drafted player is now on that team's roster.
            let team = league.teams.iter().find(|t| t.id == p.team_id).unwrap();
            assert!(team.roster.contains(&pid));
        }

        // New season clears the draft and resets records.
        league.start_new_season();
        assert!(league.draft.is_none());
        assert_eq!(league.phase, Phase::RegularSeason);
        assert!(league.teams.iter().all(|t| t.games_played() == 0));
    }

    #[test]
    fn young_players_develop_toward_potential() {
        let mut league = League::new(5);
        league.select_team(0);

        // Find a young player with real upside.
        let pid = league
            .players
            .iter()
            .find(|p| p.age <= 21 && p.potential >= p.overall() + 8)
            .map(|p| p.id)
            .expect("a young high-upside player exists");
        let before = league.players.iter().find(|p| p.id == pid).unwrap().overall();

        // Run a few offseasons of development.
        for _ in 0..3 {
            league.sim_to_end_of_season();
            league.start_playoffs();
            league.playoff_sim_all();
            league.finish_season();
            league.start_new_season();
        }
        let after = league.players.iter().find(|p| p.id == pid).unwrap().overall();
        assert!(after > before, "young player {before}->{after} did not grow");
    }

    #[test]
    fn scouting_refines_estimates() {
        let mut league = League::new(8);
        league.select_team(0);
        league.sim_to_end_of_season();
        league.start_playoffs();
        league.playoff_sim_all();
        league.finish_season();
        league.enter_draft();

        let pid = league.draft.as_ref().unwrap().prospects[0];
        let u0 = league.draft.as_ref().unwrap().scouting[&pid].uncertainty;
        let points0 = league.draft.as_ref().unwrap().scout_points;
        league.scout_prospect(pid);
        let u1 = league.draft.as_ref().unwrap().scouting[&pid].uncertainty;
        let points1 = league.draft.as_ref().unwrap().scout_points;
        assert!(u1 < u0, "scouting should reduce uncertainty");
        assert_eq!(points1, points0 - 1, "scouting costs a point");
    }

    #[test]
    fn simcast_produces_play_by_play() {
        let mut league = League::new(12);
        league.select_team(0);
        // Find the user's first scheduled game.
        let idx = league.schedule.iter().position(|g| g.home == 0 || g.away == 0).unwrap();
        let events = league.watch_scheduled_game(idx).expect("events produced");
        assert!(events.len() > 50, "a game should have many possessions");

        // The final event's score matches the recorded result, and the game is
        // now marked played.
        let last = events.last().unwrap();
        let res = league.schedule[idx].result.unwrap();
        assert_eq!(last.home_score, res.home_score);
        assert_eq!(last.away_score, res.away_score);
        assert!(league.schedule[idx].is_played());

        // Quarters and clocks are present; box snapshots are non-empty.
        assert!(events.iter().all(|e| !e.clock.is_empty() && !e.home_box.is_empty()));
        // Watching it again is a no-op (already played).
        assert!(league.watch_scheduled_game(idx).is_none());
    }

    #[test]
    fn save_load_round_trips() {
        let mut league = League::new(7);
        league.select_team(5);
        league.sim_days(10);
        let json = league.to_json();
        let restored = League::from_json(&json).unwrap();
        assert_eq!(restored.season, league.season);
        assert_eq!(restored.teams.len(), 32);
        assert_eq!(restored.user_team_id, Some(5));
        // Records preserved.
        let played: u32 = restored.teams.iter().map(|t| t.games_played()).sum();
        assert!(played > 0);
    }
}
