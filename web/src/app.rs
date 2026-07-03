//! Root component: sets up global state, theming, and routes between the
//! team builder and the main dashboard.

use crate::dashboard::Dashboard;
use crate::draft::DraftScreen;
use crate::state::{load_saved_league, load_theme, save_theme, AppState, Tab};
use crate::team_builder::TeamBuilder;
use engine::{League, Phase};
use leptos::prelude::*;

/// A seed for new leagues, derived from the page-load timestamp.
fn time_seed() -> u64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| p.now() as u64)
        .unwrap_or(12345)
        .wrapping_mul(2654435761)
        ^ 0x9E3779B9
}

#[component]
pub fn App() -> impl IntoView {
    let league = RwSignal::new(load_saved_league().unwrap_or_else(|| League::new(time_seed())));
    let dark = RwSignal::new(load_theme());
    let tab = RwSignal::new(Tab::Standings);
    let watching = RwSignal::new(None::<usize>);
    let watch_events = StoredValue::new(Vec::<engine::PlayEvent>::new());
    let viewing = RwSignal::new(None::<engine::PlayerId>);
    let state = AppState { league, dark, tab, watching, watch_events, viewing };
    provide_context(state);

    // Persist theme whenever it changes.
    Effect::new(move |_| save_theme(dark.get()));

    // Accent color comes from the user's team (falls back to a default).
    let accent = move || {
        league.with(|l| {
            l.user_team_id
                .and_then(|id| l.teams.iter().find(|t| t.id == id))
                .map(|t| t.primary.hex().to_string())
                .unwrap_or_else(|| "#f97316".to_string())
        })
    };

    let root_class = move || {
        if dark.get() {
            "app theme-dark"
        } else {
            "app theme-light"
        }
    };

    let phase = move || league.with(|l| l.phase);

    view! {
        <div class=root_class style=move || format!("--accent:{}", accent())>
            {move || match phase() {
                Phase::TeamSelect => view! { <TeamBuilder/> }.into_any(),
                Phase::Draft => view! { <DraftScreen/> }.into_any(),
                Phase::FreeAgency => view! { <crate::free_agency::FreeAgencyScreen/> }.into_any(),
                _ => view! { <Dashboard/> }.into_any(),
            }}
            <crate::player_modal::PlayerModal/>
        </div>
    }
}
