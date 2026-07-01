//! Player detail modal: opens when a player's name is clicked anywhere in the
//! app. Shows bio, bar-graph ratings grouped Hoopland-style (with the numbers
//! still visible), the current season line, a season-by-season career log, and
//! the accomplishments (honors) the player has earned.

use crate::state::AppState;
use engine::{Honor, SeasonStats};
use leptos::prelude::*;

/// One labeled 0–100 attribute for the bar display.
#[derive(Clone)]
struct RatingRow {
    label: &'static str,
    val: u8,
}

/// A named group of attributes (Inside, Outside, ...).
#[derive(Clone)]
struct Group {
    name: &'static str,
    grade: u8,
    rows: Vec<RatingRow>,
}

/// One season in the career log.
#[derive(Clone)]
struct CareerRow {
    season: u32,
    team: String,
    age: u8,
    ovr: u8,
    gp: u32,
    ppg: f64,
    rpg: f64,
    apg: f64,
    fg: f64,
    tp: f64,
}

/// Everything the modal needs, cloned out of the league under one borrow.
#[derive(Clone)]
struct PView {
    name: String,
    pos: &'static str,
    age: u8,
    team: String,
    ovr: u8,
    pot: u8,
    salary: String,
    years: u8,
    groups: Vec<Group>,
    live: Option<(u32, f64, f64, f64, f64, f64, f64)>, // gp, ppg, rpg, apg, mpg, fg, tp
    career: Vec<CareerRow>,
    totals: Option<(u32, f64, f64, f64)>, // gp, ppg, rpg, apg (career)
    honors: Vec<String>,
}

fn pergame(total: u32, gp: u32) -> f64 {
    if gp == 0 { 0.0 } else { total as f64 / gp as f64 }
}

#[component]
pub fn PlayerModal() -> impl IntoView {
    let state = expect_context::<AppState>();
    let league = state.league;
    let viewing = state.viewing;

    let close = move |_| viewing.set(None);

    let data = move || -> Option<PView> {
        let pid = viewing.get()?;
        league.with(|l| {
            let p = l.players.iter().find(|p| p.id == pid)?;
            let r = &p.ratings;
            let team = p
                .team
                .and_then(|t| l.teams.iter().find(|tm| tm.id == t))
                .map(|t| t.full_name())
                .unwrap_or_else(|| "Free Agent".to_string());

            let groups = vec![
                Group { name: "Inside", grade: r.inside(), rows: vec![
                    RatingRow { label: "Layup", val: r.layup },
                    RatingRow { label: "Dunk", val: r.dunk },
                    RatingRow { label: "Post", val: r.post },
                ]},
                Group { name: "Outside", grade: r.outside(), rows: vec![
                    RatingRow { label: "Mid-Range", val: r.mid_range },
                    RatingRow { label: "Three", val: r.three },
                    RatingRow { label: "Free Throw", val: r.free_throw },
                ]},
                Group { name: "Playmaking", grade: r.playmaking(), rows: vec![
                    RatingRow { label: "Passing", val: r.passing },
                    RatingRow { label: "Handle", val: r.ball_handling },
                    RatingRow { label: "IQ", val: r.basketball_iq },
                ]},
                Group { name: "Defense", grade: r.defending(), rows: vec![
                    RatingRow { label: "Interior D", val: r.interior_defense },
                    RatingRow { label: "Perimeter D", val: r.perimeter_defense },
                    RatingRow { label: "Steal", val: r.steal },
                    RatingRow { label: "Block", val: r.block },
                    RatingRow { label: "Rebound", val: r.rebounding },
                ]},
                Group { name: "Physical", grade: r.athletic(), rows: vec![
                    RatingRow { label: "Athleticism", val: r.athleticism },
                    RatingRow { label: "Stamina", val: r.stamina },
                ]},
            ];

            let career = l.career(pid);

            // Career season-by-season rows.
            let rows: Vec<CareerRow> = career
                .map(|c| {
                    c.seasons
                        .iter()
                        .map(|cs| {
                            let s = &cs.stats;
                            CareerRow {
                                season: cs.season,
                                team: cs.team_abbrev.clone(),
                                age: cs.age,
                                ovr: cs.overall,
                                gp: s.gp,
                                ppg: s.ppg(),
                                rpg: s.rpg(),
                                apg: s.apg(),
                                fg: s.fg_pct(),
                                tp: s.tp_pct(),
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();

            // Career totals.
            let totals = career.map(|c| c.totals()).filter(|t: &SeasonStats| t.gp > 0).map(|t| {
                (t.gp, t.ppg(), t.rpg(), t.apg())
            });

            // The in-progress season line (only if it's not already in the log).
            let st = &l.season_stats[pid as usize];
            let already_logged = rows.last().map(|r| r.season == l.season).unwrap_or(false);
            let live = if st.gp > 0 && !already_logged {
                Some((st.gp, st.ppg(), st.rpg(), st.apg(), pergame(st.min, st.gp), st.fg_pct(), st.tp_pct()))
            } else {
                None
            };

            // Honors as aggregated badges, most prestigious first.
            let honors = career
                .map(|c| {
                    let order = [Honor::Champion, Honor::Mvp, Honor::FinalsMvp, Honor::Dpoy, Honor::Roy];
                    order
                        .iter()
                        .filter_map(|h| {
                            let n = c.count(*h);
                            if n == 0 {
                                None
                            } else if n == 1 {
                                Some(h.short().to_string())
                            } else {
                                Some(format!("{}\u{00d7} {}", n, h.short()))
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();

            Some(PView {
                name: p.name.clone(),
                pos: p.position.abbrev(),
                age: p.age,
                team,
                ovr: p.overall(),
                pot: p.potential,
                salary: if p.contract.years > 0 { p.contract.salary_str() } else { "\u{2014}".into() },
                years: p.contract.years,
                groups,
                live,
                career: rows,
                totals,
                honors,
            })
        })
    };

    view! {
        <Show when=move || viewing.get().is_some()>
            {move || match data() {
                None => view! { <div></div> }.into_any(),
                Some(v) => {
                    let pct = |x: f64| format!("{:.1}%", x * 100.0);
                    view! {
                        <div class="modal-backdrop" on:click=close>
                            // Stop clicks inside the card from closing the modal.
                            <div class="modal-card player-modal" on:click=|e| e.stop_propagation()>
                                <button class="modal-close" title="Close" on:click=close>"\u{2715}"</button>

                                <div class="pm-head">
                                    <div class="pm-ovr">{v.ovr}</div>
                                    <div class="pm-id">
                                        <h2 class="pm-name">{v.name}</h2>
                                        <div class="pm-meta">
                                            {v.pos}" \u{2022} Age "{v.age}" \u{2022} "{v.team}
                                        </div>
                                        <div class="pm-sub">
                                            <span>"POT "<b>{v.pot}</b></span>
                                            <span>"Salary "<b>{v.salary}</b>
                                                {(v.years > 0).then(|| format!(" / {}yr", v.years))}
                                            </span>
                                        </div>
                                    </div>
                                </div>

                                {(!v.honors.is_empty()).then({
                                    let honors = v.honors.clone();
                                    move || view! {
                                        <div class="pm-honors">
                                            {honors.into_iter().map(|h| view! {
                                                <span class="honor-badge">{h}</span>
                                            }).collect_view()}
                                        </div>
                                    }
                                })}

                                <div class="pm-body">
                                    <div class="pm-ratings">
                                        {v.groups.into_iter().map(|g| view! {
                                            <div class="rt-group">
                                                <div class="rt-group-head">
                                                    <span class="rt-group-name">{g.name}</span>
                                                    <span class="rt-group-grade">{g.grade}</span>
                                                </div>
                                                {g.rows.into_iter().map(|row| view! {
                                                    <div class="rt-row">
                                                        <span class="rt-label">{row.label}</span>
                                                        <span class="rt-track">
                                                            <span class="rt-fill" style=format!("width:{}%", row.val)></span>
                                                        </span>
                                                        <span class="rt-val">{row.val}</span>
                                                    </div>
                                                }).collect_view()}
                                            </div>
                                        }).collect_view()}
                                    </div>

                                    <div class="pm-stats">
                                        {v.live.map(|(gp, ppg, rpg, apg, mpg, fg, tp)| view! {
                                            <div class="pm-season-now">
                                                <h4 class="pm-section">"This Season"</h4>
                                                <div class="statline">
                                                    <span><b>{format!("{:.1}", ppg)}</b>" PPG"</span>
                                                    <span><b>{format!("{:.1}", rpg)}</b>" RPG"</span>
                                                    <span><b>{format!("{:.1}", apg)}</b>" APG"</span>
                                                    <span><b>{format!("{:.1}", mpg)}</b>" MPG"</span>
                                                    <span><b>{pct(fg)}</b>" FG"</span>
                                                    <span><b>{pct(tp)}</b>" 3P"</span>
                                                    <span class="dim">{gp}" GP"</span>
                                                </div>
                                            </div>
                                        })}

                                        <h4 class="pm-section">"Career"</h4>
                                        {if v.career.is_empty() {
                                            view! { <p class="empty">"No completed seasons yet."</p> }.into_any()
                                        } else {
                                            let totals = v.totals;
                                            view! {
                                                <table class="tbl pm-career">
                                                    <thead><tr>
                                                        <th>"Yr"</th><th>"Tm"</th><th>"Age"</th><th>"OVR"</th><th>"GP"</th>
                                                        <th>"PPG"</th><th>"RPG"</th><th>"APG"</th><th>"FG"</th><th>"3P"</th>
                                                    </tr></thead>
                                                    <tbody>
                                                        {v.career.into_iter().map(|r| view! {
                                                            <tr class="row">
                                                                <td>{r.season}</td><td>{r.team}</td><td>{r.age}</td>
                                                                <td><span class="ovr">{r.ovr}</span></td><td>{r.gp}</td>
                                                                <td>{format!("{:.1}", r.ppg)}</td>
                                                                <td>{format!("{:.1}", r.rpg)}</td>
                                                                <td>{format!("{:.1}", r.apg)}</td>
                                                                <td>{pct(r.fg)}</td>
                                                                <td>{pct(r.tp)}</td>
                                                            </tr>
                                                        }).collect_view()}
                                                    </tbody>
                                                    {totals.map(|(gp, ppg, rpg, apg)| view! {
                                                        <tfoot><tr class="pm-totals">
                                                            <td colspan="4">"Career"</td><td>{gp}</td>
                                                            <td>{format!("{:.1}", ppg)}</td>
                                                            <td>{format!("{:.1}", rpg)}</td>
                                                            <td>{format!("{:.1}", apg)}</td>
                                                            <td colspan="2"></td>
                                                        </tr></tfoot>
                                                    })}
                                                </table>
                                            }.into_any()
                                        }}
                                    </div>
                                </div>
                            </div>
                        </div>
                    }.into_any()
                }
            }}
        </Show>
    }
}
