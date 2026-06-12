//! Basketball GM game engine.
//!
//! Pure Rust, no UI and no web dependencies. The whole game lives here so the
//! web layer can stay a thin presentation shell. The central type is
//! [`League`], which is fully serializable for save/load.

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

pub use league::{League, Phase, PlayoffOutcome, SeasonHistory, SeasonRecap};
pub use player::{Player, Ratings};
pub use playoffs::{Playoffs, Series, ROUND_NAMES};
pub use schedule::{Game, GameResult};
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

        // Run the playoffs.
        league.start_playoffs();
        assert_eq!(league.phase, Phase::Playoffs);
        let po = league.playoffs.as_ref().unwrap();
        assert!(po.champion.is_some());

        // Recap and finish.
        let recap = league.season_recap().expect("recap exists");
        assert_eq!(recap.team_name, "Atlanta Test Team");
        assert!(recap.wins + recap.losses == 82);
        league.finish_season();
        assert_eq!(league.phase, Phase::Offseason);
        assert_eq!(league.history.len(), 1);
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
