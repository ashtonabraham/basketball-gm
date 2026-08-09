//! Root component: sets up global state, theming, and routes between the home
//! (save slots) screen, the team builder, and the main dashboard.

use crate::dashboard::Dashboard;
use crate::draft::DraftScreen;
use crate::home::HomeScreen;
use crate::state::{load_current, load_slot, load_theme, save_theme, AppState, Tab};
use crate::team_builder::TeamBuilder;
use engine::{League, Phase};
use leptos::prelude::*;

/// Build version, shown in the corner so you can tell the deployed build apart.
pub const VERSION: &str = "v0.9.2";

/// A seed for new leagues, derived from the page-load timestamp.
pub fn time_seed() -> u64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| p.now() as u64)
        .unwrap_or(12345)
        .wrapping_mul(2654435761)
        ^ 0x9E3779B9
}

#[component]
pub fn App() -> impl IntoView {
    // Resume the last-played slot if there is one; otherwise start on the home
    // screen with a throwaway league in the signal until a slot is opened.
    let start_slot = load_current();
    let start_league = start_slot.and_then(load_slot).unwrap_or_else(|| League::new(time_seed()));
    let resume = start_slot.filter(|id| load_slot(*id).is_some());

    let league = RwSignal::new(start_league);
    let dark = RwSignal::new(load_theme());
    let tab = RwSignal::new(Tab::Standings);
    let current_slot = RwSignal::new(resume);
    let watching = RwSignal::new(None::<usize>);
    let watch_events = StoredValue::new(Vec::<engine::PlayEvent>::new());
    let viewing = RwSignal::new(None::<engine::PlayerId>);
    let viewing_team = RwSignal::new(None::<engine::TeamId>);
    let state = AppState { league, dark, tab, current_slot, watching, watch_events, viewing, viewing_team };
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
            {move || {
                // No active slot -> home / save-slots screen.
                if current_slot.get().is_none() {
                    return view! { <HomeScreen/> }.into_any();
                }
                match phase() {
                    Phase::TeamSelect => view! { <TeamBuilder/> }.into_any(),
                    Phase::Draft => view! { <DraftScreen/> }.into_any(),
                    Phase::FreeAgency => view! { <crate::free_agency::FreeAgencyScreen/> }.into_any(),
                    _ => view! { <Dashboard/> }.into_any(),
                }
            }}
            <crate::dashboard::TeamModal/>
            <crate::player_modal::PlayerModal/>
            <crate::dashboard::GoalPopup/>
            <crate::dashboard::PlayerEventPopup/>
            <crate::dashboard::FiredOverlay/>
            <div class="version-badge" title="Build version">{VERSION}</div>
        </div>
    }
}
