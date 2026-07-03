//! The "simcast": watch a scheduled game play out possession-by-possession with
//! a live scoreboard, play-by-play feed, and a live box score.

use crate::state::AppState;
use leptos::prelude::*;
use std::collections::HashMap;
use std::time::Duration;

#[component]
pub fn SimcastOverlay() -> impl IntoView {
    let state = expect_context::<AppState>();
    let idx = state.watching.get_untracked().unwrap_or(usize::MAX);

    // The game was already simmed once in the click handler; we just replay the
    // stored events visually here (no league mutation during render).
    let events = state.watch_events.get_value();

    let len = events.len();
    if len == 0 {
        state.watching.set(None);
        return view! { <div></div> }.into_any();
    }

    // Team identities + a player-id -> name map (static for the game).
    let (home_id, away_id) = state.league.with_untracked(|l| { let g = &l.schedule[idx]; (g.home, g.away) });
    let team_info = |id: u32| state.league.with_untracked(|l| {
        l.teams.iter().find(|t| t.id == id)
            .map(|t| (t.abbrev.clone(), t.primary.hex().to_string(), t.secondary.hex().to_string()))
            .unwrap_or_default()
    });
    let (h_ab, h_c1, h_c2) = team_info(home_id);
    let (a_ab, a_c1, a_c2) = team_info(away_id);
    let names: HashMap<u32, String> = state.league.with_untracked(|l| l.players.iter().map(|p| (p.id, p.name.clone())).collect());
    let names = StoredValue::new(names);
    let events = StoredValue::new(events);

    let cursor = RwSignal::new(0usize);
    let playing = RwSignal::new(true);
    let speed = RwSignal::new(1usize);

    // Playback timer.
    let handle = set_interval_with_handle(
        move || {
            if playing.get_untracked() {
                cursor.update(|c| *c = (*c + speed.get_untracked()).min(len));
                if cursor.get_untracked() >= len {
                    playing.set(false);
                }
            }
        },
        Duration::from_millis(140),
    ).ok();
    on_cleanup(move || { if let Some(h) = handle { h.clear(); } });

    // Current scoreboard state (from the last played event).
    let score = move || events.with_value(|ev| {
        let c = cursor.get();
        if c == 0 { (0u32, 0u32, 1u8, "12:00".to_string()) }
        else { let e = &ev[c - 1]; (e.home_score, e.away_score, e.quarter, e.clock.clone()) }
    });
    // Play-by-play feed (newest first).
    let feed = move || events.with_value(|ev| {
        let c = cursor.get();
        let start = c.saturating_sub(18);
        ev[start..c].iter().rev().map(|e| (e.quarter, e.clock.clone(), e.text.clone(), e.scored, e.team_id == home_id)).collect::<Vec<_>>()
    });
    // Live box score for one side: top scorers (name, pts, reb, ast).
    let box_rows = move |home: bool| events.with_value(|ev| {
        let c = cursor.get();
        let lines = if c == 0 { Vec::new() } else {
            let e = &ev[c - 1];
            let b = if home { &e.home_box } else { &e.away_box };
            b.clone()
        };
        let mut rows: Vec<(String, u32, u32, u32)> = lines.into_iter()
            .map(|l| (names.with_value(|m| m.get(&l.player_id).cloned().unwrap_or_default()), l.pts, l.oreb + l.dreb, l.ast))
            .collect();
        rows.sort_by(|x, y| y.1.cmp(&x.1));
        rows.truncate(6);
        rows
    });

    let toggle = move |_| playing.update(|p| *p = !*p);
    let skip = move |_| { cursor.set(len); playing.set(false); };
    let close = move |_| state.watching.set(None);
    let set_speed = move |s: usize| speed.set(s);

    let spd_btn = move |s: usize, label: &'static str| view! {
        <button class=move || if speed.get() == s { "sim-spd active" } else { "sim-spd" }
            on:click=move |_| set_speed(s)>{label}</button>
    };

    let box_table = move |home: bool, ab: String| view! {
        <div class="sim-box">
            <div class="sim-box-team">{ab}</div>
            <table class="tbl">
                <thead><tr><th class="left">"Player"</th><th>"PTS"</th><th>"REB"</th><th>"AST"</th></tr></thead>
                <tbody>
                    {move || box_rows(home).into_iter().map(|(n, p, r, a)| view! {
                        <tr class="row"><td class="left">{n}</td><td><b>{p}</b></td><td>{r}</td><td>{a}</td></tr>
                    }).collect_view()}
                </tbody>
            </table>
        </div>
    };

    view! {
        <div class="overlay sim-overlay">
            <div class="sim-card">
                <div class="sim-scoreboard">
                    <div class="sim-team">
                        <span class="mini-logo" style=format!("--c1:{};--c2:{}", a_c1, a_c2)>{a_ab.clone()}</span>
                        <span class="sim-abbr">{a_ab.clone()}</span>
                        <span class="sim-score">{move || score().1}</span>
                    </div>
                    <div class="sim-center">
                        <div class="sim-clock">{move || { let (_, _, q, c) = score(); if q >= 5 { "OT".to_string() } else { format!("Q{} {}", q, c) } }}</div>
                        <button class="sim-close" on:click=close>"\u{2715} Close"</button>
                    </div>
                    <div class="sim-team">
                        <span class="sim-score">{move || score().0}</span>
                        <span class="sim-abbr">{h_ab.clone()}</span>
                        <span class="mini-logo" style=format!("--c1:{};--c2:{}", h_c1, h_c2)>{h_ab.clone()}</span>
                    </div>
                </div>

                <div class="sim-body">
                    <div class="sim-feed">
                        {move || feed().into_iter().map(|(q, clk, text, scored, is_home)| {
                            let cls = if scored { "sim-play scored" } else { "sim-play" };
                            view! {
                                <div class=cls>
                                    <span class="sim-play-clk">{format!("Q{} {}", q, clk)}</span>
                                    <span class=if is_home { "sim-tag home" } else { "sim-tag away" }></span>
                                    <span class="sim-play-text">{text}</span>
                                </div>
                            }
                        }).collect_view()}
                    </div>
                    <div class="sim-boxes">
                        {box_table(false, a_ab.clone())}
                        {box_table(true, h_ab.clone())}
                    </div>
                </div>

                <div class="sim-controls">
                    <button class="btn btn-primary" on:click=toggle>
                        {move || if playing.get() { "\u{23f8} Pause" } else { "\u{25b6} Play" }}
                    </button>
                    <div class="sim-speeds">
                        {spd_btn(1, "1x")}{spd_btn(2, "2x")}{spd_btn(4, "4x")}{spd_btn(8, "8x")}
                    </div>
                    <button class="btn" on:click=skip>"\u{23ed} Skip to end"</button>
                    <span class="sim-progress">{move || format!("{} / {}", cursor.get(), len)}</span>
                </div>
            </div>
        </div>
    }.into_any()
}
