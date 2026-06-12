//! Standings: ordering teams within a conference.

use crate::team::Team;
use crate::types::{Conference, TeamId};

/// A team's line in the standings table.
#[derive(Debug, Clone)]
pub struct StandingsRow {
    pub seed: u32,
    pub team_id: TeamId,
    pub wins: u32,
    pub losses: u32,
    pub win_pct: f64,
}

/// Return the standings for one conference, best record first, seeded 1..=16.
pub fn conference_standings(teams: &[Team], conf: Conference) -> Vec<StandingsRow> {
    let mut rows: Vec<&Team> = teams.iter().filter(|t| t.conference == conf).collect();
    rows.sort_by(|a, b| {
        b.win_pct()
            .partial_cmp(&a.win_pct())
            .unwrap()
            .then(b.wins.cmp(&a.wins))
            .then(a.full_name().cmp(&b.full_name()))
    });
    rows.into_iter()
        .enumerate()
        .map(|(i, t)| StandingsRow {
            seed: i as u32 + 1,
            team_id: t.id,
            wins: t.wins,
            losses: t.losses,
            win_pct: t.win_pct(),
        })
        .collect()
}

/// The top-8 seeds of a conference (the playoff teams), in seed order.
pub fn playoff_seeds(teams: &[Team], conf: Conference) -> Vec<TeamId> {
    conference_standings(teams, conf)
        .into_iter()
        .take(8)
        .map(|r| r.team_id)
        .collect()
}
