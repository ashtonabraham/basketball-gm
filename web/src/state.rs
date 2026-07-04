//! Shared app state, theme, and browser persistence.

use engine::{League, PlayEvent, PlayerId, TeamId};
use leptos::prelude::*;

const SAVE_KEY: &str = "hardwood_gm_save";
const THEME_KEY: &str = "hardwood_gm_theme";

/// Which content panel is showing in the dashboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Standings,
    Schedule,
    Roster,
    Stats,
    Trades,
    Finances,
    Playoffs,
    History,
}

/// Global, `Copy`-able handle to all reactive app state. Provided via context.
#[derive(Clone, Copy)]
pub struct AppState {
    pub league: RwSignal<League>,
    pub dark: RwSignal<bool>,
    pub tab: RwSignal<Tab>,
    /// Schedule index of the game being watched in the simcast, if any.
    pub watching: RwSignal<Option<usize>>,
    /// Play-by-play for the game being watched, computed once when it opens.
    pub watch_events: StoredValue<Vec<PlayEvent>>,
    /// Id of the player whose detail modal is open, if any.
    pub viewing: RwSignal<Option<PlayerId>>,
    /// Id of the team whose roster modal is open, if any.
    pub viewing_team: RwSignal<Option<TeamId>>,
}

impl AppState {
    /// Mutate the league and persist the result to localStorage.
    pub fn update_league(&self, f: impl FnOnce(&mut League)) {
        self.league.update(f);
        save_league(&self.league.get_untracked());
    }

    /// Mutate the league reactively WITHOUT serializing to localStorage. Use for
    /// high-frequency updates (e.g. dragging a slider); call `persist` on release.
    pub fn update_league_quiet(&self, f: impl FnOnce(&mut League)) {
        self.league.update(f);
    }

    /// Write the current league to localStorage.
    pub fn persist(&self) {
        save_league(&self.league.get_untracked());
    }
}

/// Read the browser's localStorage, if available.
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

pub fn save_league(league: &League) {
    if let Some(store) = local_storage() {
        let _ = store.set_item(SAVE_KEY, &league.to_json());
    }
}

pub fn load_saved_league() -> Option<League> {
    let json = local_storage()?.get_item(SAVE_KEY).ok().flatten()?;
    League::from_json(&json).ok()
}

#[allow(dead_code)] // used by an upcoming "New League" reset button
pub fn clear_save() {
    if let Some(store) = local_storage() {
        let _ = store.remove_item(SAVE_KEY);
    }
}

pub fn save_theme(dark: bool) {
    if let Some(store) = local_storage() {
        let _ = store.set_item(THEME_KEY, if dark { "dark" } else { "light" });
    }
}

pub fn load_theme() -> bool {
    // Default to dark.
    local_storage()
        .and_then(|s| s.get_item(THEME_KEY).ok().flatten())
        .map(|v| v != "light")
        .unwrap_or(true)
}
