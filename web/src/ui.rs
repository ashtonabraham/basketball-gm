//! Small shared UI pieces.

use crate::state::AppState;
use leptos::prelude::*;

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

/// Format a win-loss record's percentage like ".634".
pub fn fmt_pct(pct: f64) -> String {
    let s = format!("{:.3}", pct);
    // Drop a leading zero: 0.634 -> .634
    s.strip_prefix('0').map(|r| r.to_string()).unwrap_or(s)
}
