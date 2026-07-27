//! Home screen: the save-slot manager. Create a new league (its own save),
//! resume an existing one, or delete a save.

use crate::state::{delete_slot, list_slots, AppState};
use crate::ui::ThemeToggle;
use leptos::prelude::*;

#[component]
pub fn HomeScreen() -> impl IntoView {
    let state = expect_context::<AppState>();

    // Reactive list of saves; a version counter forces a refresh after create/delete.
    let refresh = RwSignal::new(0u32);
    let slots = move || { refresh.get(); list_slots() };

    let name = RwSignal::new(String::new());
    let create = move |_| {
        let raw = name.get_untracked();
        let trimmed = raw.trim();
        let n = if trimmed.is_empty() { "New League".to_string() } else { trimmed.to_string() };
        state.new_game(&n, crate::app::time_seed());
    };
    let open = move |id: u32| state.open_slot(id);
    let remove = move |id: u32| {
        delete_slot(id);
        refresh.update(|n| *n += 1);
    };

    view! {
        <div class="home">
            <header class="home-top">
                <h1 class="brand">"The " <span class="brand-accent">"League"</span></h1>
                <ThemeToggle/>
            </header>
            <p class="subtitle">"Run a franchise. Create a new league or jump back into one of your saves."</p>

            <div class="home-new card">
                <h3 class="card-title">"New League"</h3>
                <div class="home-new-row">
                    <input class="input" type="text" placeholder="League name (optional)"
                        prop:value=move || name.get()
                        on:input=move |e| name.set(event_target_value(&e))
                        on:keydown=move |e| if e.key() == "Enter" { create(()) }/>
                    <button class="btn btn-primary" on:click=move |_| create(())>"Create \u{2192}"</button>
                </div>
            </div>

            <div class="card" style="margin-top:1.25rem">
                <h3 class="card-title">"Your Saves"</h3>
                {move || {
                    let s = slots();
                    if s.is_empty() {
                        view! { <p class="empty">"No saved leagues yet. Create one above to get started."</p> }.into_any()
                    } else {
                        view! {
                            <div class="slot-list">
                                {s.into_iter().map(|slot| {
                                    let id = slot.id;
                                    view! {
                                        <div class="slot-row">
                                            <div class="slot-info">
                                                <div class="slot-name">{slot.name}</div>
                                                <div class="slot-meta">{format!("Season {} \u{2022} {}", slot.season, slot.team)}</div>
                                            </div>
                                            <div class="slot-actions">
                                                <button class="btn btn-primary" on:click=move |_| open(id)>"Play"</button>
                                                <button class="btn slot-del" title="Delete save"
                                                    on:click=move |_| remove(id)>"\u{1f5d1}"</button>
                                            </div>
                                        </div>
                                    }
                                }).collect_view()}
                            </div>
                        }.into_any()
                    }
                }}
            </div>
        </div>
    }
}
