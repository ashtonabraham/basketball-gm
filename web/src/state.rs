//! Shared app state, theme, and browser persistence (multi-slot save system).

use engine::{League, PlayEvent, PlayerId, TeamId};
use leptos::prelude::*;

const THEME_KEY: &str = "hardwood_gm_theme";
/// Per-slot league JSON is stored under `tl_slot_{id}`.
const SLOT_PREFIX: &str = "tl_slot_";
/// A slot's display name is stored under `tl_slotname_{id}`.
const SLOT_NAME_PREFIX: &str = "tl_slotname_";
/// The id of the slot currently being played.
const CURRENT_KEY: &str = "tl_current";

/// Which content panel is showing in the dashboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Standings,
    Schedule,
    Roster,
    Stats,
    Trades,
    Finances,
    Owner,
    Playoffs,
    History,
}

/// Summary of one saved league, for the home/slots screen.
#[derive(Clone)]
pub struct SlotInfo {
    pub id: u32,
    pub name: String,
    pub season: u32,
    pub team: String,
}

/// Global, `Copy`-able handle to all reactive app state. Provided via context.
#[derive(Clone, Copy)]
pub struct AppState {
    pub league: RwSignal<League>,
    pub dark: RwSignal<bool>,
    pub tab: RwSignal<Tab>,
    /// The save slot currently loaded; `None` shows the home/slots screen.
    pub current_slot: RwSignal<Option<u32>>,
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
    /// Mutate the league and persist it to the current slot.
    pub fn update_league(&self, f: impl FnOnce(&mut League)) {
        self.league.update(f);
        self.persist();
    }

    /// Mutate the league reactively WITHOUT serializing. Use for high-frequency
    /// updates (e.g. dragging a slider); call `persist` on release.
    pub fn update_league_quiet(&self, f: impl FnOnce(&mut League)) {
        self.league.update(f);
    }

    /// Write the current league to its save slot.
    pub fn persist(&self) {
        if let Some(id) = self.current_slot.get_untracked() {
            save_slot(id, &self.league.get_untracked());
        }
    }

    /// Load a slot's league and make it the active game.
    pub fn open_slot(&self, id: u32) {
        if let Some(league) = load_slot(id) {
            self.league.set(league);
            self.current_slot.set(Some(id));
            self.tab.set(Tab::Standings);
            set_current(Some(id));
        }
    }

    /// Create a fresh league in a new slot and open it (starts in team select).
    pub fn new_game(&self, name: &str, seed: u64) {
        let id = next_slot_id();
        let league = League::new(seed);
        save_slot(id, &league);
        set_slot_name(id, name);
        self.league.set(league);
        self.current_slot.set(Some(id));
        self.tab.set(Tab::Standings);
        set_current(Some(id));
    }

    /// Return to the home/slots screen (keeps the save).
    pub fn go_home(&self) {
        self.persist();
        self.current_slot.set(None);
        set_current(None);
    }
}

/// Read the browser's localStorage, if available.
fn local_storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

fn slot_key(id: u32) -> String {
    format!("{SLOT_PREFIX}{id}")
}
fn slot_name_key(id: u32) -> String {
    format!("{SLOT_NAME_PREFIX}{id}")
}

pub fn save_slot(id: u32, league: &League) {
    if let Some(store) = local_storage() {
        let _ = store.set_item(&slot_key(id), &league.to_json());
    }
}

pub fn load_slot(id: u32) -> Option<League> {
    let json = local_storage()?.get_item(&slot_key(id)).ok().flatten()?;
    League::from_json(&json).ok()
}

pub fn delete_slot(id: u32) {
    if let Some(store) = local_storage() {
        let _ = store.remove_item(&slot_key(id));
        let _ = store.remove_item(&slot_name_key(id));
    }
}

fn set_slot_name(id: u32, name: &str) {
    if let Some(store) = local_storage() {
        let _ = store.set_item(&slot_name_key(id), name);
    }
}
fn slot_name(id: u32) -> Option<String> {
    local_storage()?.get_item(&slot_name_key(id)).ok().flatten()
}

pub fn set_current(id: Option<u32>) {
    if let Some(store) = local_storage() {
        match id {
            Some(i) => { let _ = store.set_item(CURRENT_KEY, &i.to_string()); }
            None => { let _ = store.remove_item(CURRENT_KEY); }
        }
    }
}

pub fn load_current() -> Option<u32> {
    local_storage()?.get_item(CURRENT_KEY).ok().flatten()?.parse().ok()
}

/// All existing slot ids, ascending.
fn slot_ids() -> Vec<u32> {
    let mut ids = Vec::new();
    if let Some(store) = local_storage() {
        let n = store.length().unwrap_or(0);
        for i in 0..n {
            if let Ok(Some(key)) = store.key(i) {
                if let Some(rest) = key.strip_prefix(SLOT_PREFIX) {
                    if let Ok(id) = rest.parse::<u32>() {
                        ids.push(id);
                    }
                }
            }
        }
    }
    ids.sort_unstable();
    ids
}

fn next_slot_id() -> u32 {
    slot_ids().into_iter().max().map(|m| m + 1).unwrap_or(0)
}

/// Summaries of every saved league, for the home screen.
pub fn list_slots() -> Vec<SlotInfo> {
    slot_ids()
        .into_iter()
        .filter_map(|id| {
            let league = load_slot(id)?;
            let team = league
                .user_team_id
                .and_then(|tid| league.teams.iter().find(|t| t.id == tid))
                .map(|t| t.full_name())
                .unwrap_or_else(|| "New franchise".to_string());
            let name = slot_name(id).unwrap_or_else(|| format!("League {}", id + 1));
            Some(SlotInfo { id, name, season: league.season, team })
        })
        .collect()
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
