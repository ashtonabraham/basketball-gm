//! Team builder screen: pick one of the 32 preset franchises, then customize
//! its name, abbreviation, and jersey colors before starting the season.

use crate::state::AppState;
use crate::ui::ThemeToggle;
use engine::{Color, TeamId};
use leptos::prelude::*;

#[component]
pub fn TeamBuilder() -> impl IntoView {
    let state = expect_context::<AppState>();
    // Stable list of team ids; cards read their own data reactively.
    let team_ids: Vec<TeamId> =
        state.league.with_untracked(|l| l.teams.iter().map(|t| t.id).collect());
    let selected = RwSignal::new(None::<TeamId>);

    view! {
        <div class="builder">
            <header class="builder-top">
                <div>
                    <h1 class="brand">"Hardwood " <span class="brand-accent">"GM"</span></h1>
                    <p class="subtitle">"Choose your franchise. Location is fixed — make the name and colors yours."</p>
                </div>
                <ThemeToggle/>
            </header>

            <div class="team-grid">
                {team_ids
                    .into_iter()
                    .map(|id| view! { <TeamCard id=id selected=selected/> })
                    .collect_view()}
            </div>

            <Show when=move || selected.get().is_some()>
                <Editor id=selected.get().unwrap()/>
            </Show>
        </div>
    }
}

/// Read a single field of a team reactively.
fn team_field<T: 'static>(state: &AppState, id: TeamId, f: impl Fn(&engine::Team) -> T) -> T {
    state
        .league
        .with(|l| l.teams.iter().find(|t| t.id == id).map(&f).unwrap())
}

#[component]
fn TeamCard(id: TeamId, selected: RwSignal<Option<TeamId>>) -> impl IntoView {
    let state = expect_context::<AppState>();
    let location = team_field(&state, id, |t| t.location.clone());
    let name = move || team_field(&state, id, |t| t.name.clone());
    let primary = move || team_field(&state, id, |t| t.primary.hex().to_string());
    let secondary = move || team_field(&state, id, |t| t.secondary.hex().to_string());
    let is_sel = move || selected.get() == Some(id);

    view! {
        <button
            class=move || if is_sel() { "team-card selected" } else { "team-card" }
            style=move || format!(
                "--c1:{};--c2:{}", primary(), secondary()
            )
            on:click=move |_| selected.set(Some(id))
        >
            <span class="jersey-dot"></span>
            <span class="team-card-loc">{location.clone()}</span>
            <span class="team-card-name">{name}</span>
        </button>
    }
}

#[component]
fn Editor(id: TeamId) -> impl IntoView {
    let state = expect_context::<AppState>();
    let name = move || team_field(&state, id, |t| t.name.clone());
    let abbrev = move || team_field(&state, id, |t| t.abbrev.clone());
    let location = move || team_field(&state, id, |t| t.location.clone());
    let primary = move || team_field(&state, id, |t| t.primary.hex().to_string());
    let secondary = move || team_field(&state, id, |t| t.secondary.hex().to_string());

    let set_name = move |v: String| state.update_league(|l| l.customize_team(id, Some(v), None, None, None));
    let set_abbrev = move |v: String| {
        let v = v.to_uppercase().chars().take(3).collect::<String>();
        state.update_league(|l| l.customize_team(id, None, Some(v), None, None));
    };
    let set_primary = move |v: String| state.update_league(|l| l.customize_team(id, None, None, Some(Color::new(&v)), None));
    let set_secondary = move |v: String| state.update_league(|l| l.customize_team(id, None, None, None, Some(Color::new(&v))));

    let start = move |_| state.update_league(|l| l.select_team(id));

    view! {
        <div class="editor">
            <div class="editor-grid">
                <div class="field">
                    <label>"Location"</label>
                    <input class="input locked" prop:value=location disabled=true/>
                </div>
                <div class="field">
                    <label>"Team name"</label>
                    <input
                        class="input"
                        prop:value=name
                        on:input=move |e| set_name(event_target_value(&e))
                    />
                </div>
                <div class="field narrow">
                    <label>"Abbrev"</label>
                    <input
                        class="input"
                        maxlength="3"
                        prop:value=abbrev
                        on:input=move |e| set_abbrev(event_target_value(&e))
                    />
                </div>
                <div class="field narrow">
                    <label>"Primary"</label>
                    <input
                        class="color"
                        type="color"
                        prop:value=primary
                        on:input=move |e| set_primary(event_target_value(&e))
                    />
                </div>
                <div class="field narrow">
                    <label>"Secondary"</label>
                    <input
                        class="color"
                        type="color"
                        prop:value=secondary
                        on:input=move |e| set_secondary(event_target_value(&e))
                    />
                </div>
            </div>

            <div class="editor-foot">
                <div
                    class="jersey-preview"
                    style=move || format!("--c1:{};--c2:{}", primary(), secondary())
                >
                    <span class="jersey-abbrev">{abbrev}</span>
                </div>
                <button class="btn btn-primary" on:click=start>
                    "Start Dynasty as the " {move || format!("{} {}", location(), name())} " \u{2192}"
                </button>
            </div>
        </div>
    }
}
