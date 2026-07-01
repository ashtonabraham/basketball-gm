//! Hardwood GM — web front-end (Leptos, client-side rendered).
//! All game logic lives in the `engine` crate; this layer only presents it.

mod app;
mod dashboard;
mod draft;
mod free_agency;
mod player_modal;
mod simcast;
mod state;
mod team_builder;
mod ui;

use leptos::prelude::*;

fn main() {
    console_error_panic_hook::set_once();
    mount_to_body(app::App);
}
