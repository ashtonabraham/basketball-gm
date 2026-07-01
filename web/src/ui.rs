//! Small shared UI pieces.

use crate::state::AppState;
use engine::PlayerId;
use leptos::prelude::*;

/// A player's name rendered as a clickable link that opens the detail modal.
/// Stops propagation so it works inside rows that have their own click handler.
#[component]
pub fn PlayerLink(id: PlayerId, name: String) -> impl IntoView {
    let state = expect_context::<AppState>();
    view! {
        <span class="plink" on:click=move |e| {
            e.stop_propagation();
            state.viewing.set(Some(id));
        }>{name}</span>
    }
}

/// Sun/moon button that flips between dark and light themes.
#[component]
pub fn ThemeToggle() -> impl IntoView {
    let state = expect_context::<AppState>();
    let dark = state.dark;
    view! {
        <button
            class="theme-toggle"
            title="Toggle theme"
            on:click=move |_| dark.update(|d| *d = !*d)
        >
            {move || if dark.get() { "\u{2600}\u{fe0f}" } else { "\u{1f319}" }}
        </button>
    }
}

/// A compact view of the user's current roster, for the draft / FA screens.
#[component]
pub fn MyRosterCard() -> impl IntoView {
    let state = expect_context::<AppState>();
    let rows = move || state.league.with(|l| {
        let Some(id) = l.user_team_id else { return (Vec::new(), 0.0, 0) };
        let Some(team) = l.teams.iter().find(|t| t.id == id) else { return (Vec::new(), 0.0, 0) };
        let mut ps: Vec<_> = team.roster.iter()
            .filter_map(|pid| l.players.iter().find(|p| p.id == *pid))
            .map(|p| (p.id, p.name.clone(), p.position.abbrev(), p.age, p.overall(), p.contract.salary_str()))
            .collect();
        ps.sort_by(|a, b| b.4.cmp(&a.4));
        let space = l.team_cap_space(id) as f64 / 1000.0;
        (ps, space, team.roster.len())
    });

    view! {
        <div class="card" style="margin-top:1.25rem">
            <div class="roster-head">
                <h3 class="card-title">"My Roster"</h3>
                {move || { let (_, space, n) = rows(); view! {
                    <span class="cap-summary">
                        <span>{format!("{} players", n)}</span>
                        <span class=if space < 0.0 { "cap-over" } else { "cap-room" }>
                            {format!("{} ${:.1}M", if space < 0.0 { "Over by" } else { "Room:" }, space.abs())}
                        </span>
                    </span>
                }}}
            </div>
            <table class="tbl">
                <thead><tr><th class="left">"Player"</th><th>"Pos"</th><th>"Age"</th><th>"OVR"</th><th>"Salary"</th></tr></thead>
                <tbody>
                    {move || rows().0.into_iter().map(|(id, name, pos, age, ovr, sal)| view! {
                        <tr class="row"><td class="left"><PlayerLink id=id name=name/></td><td>{pos}</td><td>{age}</td>
                            <td><span class="ovr">{ovr}</span></td><td>{sal}</td></tr>
                    }).collect_view()}
                </tbody>
            </table>
        </div>
    }
}

/// Format a win-loss record's percentage like ".634".
pub fn fmt_pct(pct: f64) -> String {
    let s = format!("{:.3}", pct);
    // Drop a leading zero: 0.634 -> .634
    s.strip_prefix('0').map(|r| r.to_string()).unwrap_or(s)
}
