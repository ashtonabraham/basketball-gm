//! Player detail modal: opens when a player's name is clicked anywhere in the
//! app. Three tabs inside the box — Attributes (bar-graph ratings), Stats
//! (detailed per-game + season-by-season career), and Awards (accomplishments).

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

/// One season in the career log (rich).
#[derive(Clone)]
struct CareerRow {
    season: u32,
    team: String,
    age: u8,
    ovr: u8,
    gp: u32,
    mpg: f64,
    ppg: f64,
    rpg: f64,
    apg: f64,
    spg: f64,
    bpg: f64,
    fg: f64,
    tp: f64,
    ft: f64,
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
    live: Option<Vec<(&'static str, String)>>, // this-season detailed stat pairs
    career: Vec<CareerRow>,
    totals: Option<Vec<(&'static str, String)>>, // career detailed stat pairs
    honors_badges: Vec<String>,
    honors_list: Vec<(u32, &'static str)>,
}

fn pct(x: f64) -> String {
    format!("{:.1}%", x * 100.0)
}

/// Detailed per-game stat pairs for a stat block (used for both the live season
/// and career totals).
fn detail_pairs(s: &SeasonStats) -> Vec<(&'static str, String)> {
    vec![
        ("PPG", format!("{:.1}", s.ppg())),
        ("RPG", format!("{:.1}", s.rpg())),
        ("APG", format!("{:.1}", s.apg())),
        ("SPG", format!("{:.1}", s.spg())),
        ("BPG", format!("{:.1}", s.bpg())),
        ("TOV", format!("{:.1}", s.tovpg())),
        ("MPG", format!("{:.1}", s.mpg())),
        ("OREB", format!("{:.1}", s.orpg())),
        ("DREB", format!("{:.1}", s.drpg())),
        ("FG%", pct(s.fg_pct())),
        ("3P%", pct(s.tp_pct())),
        ("FT%", pct(s.ft_pct())),
        ("GP", format!("{}", s.gp)),
    ]
}

#[component]
pub fn PlayerModal() -> impl IntoView {
    let state = expect_context::<AppState>();
    let league = state.league;
    let viewing = state.viewing;

    // Which tab is showing; reset to Attributes each time a new player opens.
    let tab = RwSignal::new(0u8);
    Effect::new(move |_| {
        viewing.get();
        tab.set(0);
    });

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
                                mpg: s.mpg(),
                                ppg: s.ppg(),
                                rpg: s.rpg(),
                                apg: s.apg(),
                                spg: s.spg(),
                                bpg: s.bpg(),
                                fg: s.fg_pct(),
                                tp: s.tp_pct(),
                                ft: s.ft_pct(),
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();

            let totals = career
                .map(|c| c.totals())
                .filter(|t: &SeasonStats| t.gp > 0)
                .map(|t| detail_pairs(&t));

            // The in-progress season line (only if not already in the log).
            let st = &l.season_stats[pid as usize];
            let already_logged = rows.last().map(|r| r.season == l.season).unwrap_or(false);
            let live = (st.gp > 0 && !already_logged).then(|| detail_pairs(st));

            // Honors — aggregated badges + a dated list.
            let (honors_badges, honors_list) = career
                .map(|c| {
                    let order = [Honor::Champion, Honor::Mvp, Honor::FinalsMvp, Honor::Dpoy, Honor::Roy];
                    let badges = order
                        .iter()
                        .filter_map(|h| {
                            let n = c.count(*h);
                            match n {
                                0 => None,
                                1 => Some(h.short().to_string()),
                                _ => Some(format!("{}\u{00d7} {}", n, h.short())),
                            }
                        })
                        .collect::<Vec<_>>();
                    let mut list: Vec<(u32, &'static str)> =
                        c.honors.iter().map(|h| (h.season, h.honor.label())).collect();
                    list.sort_by(|a, b| b.0.cmp(&a.0));
                    (badges, list)
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
                honors_badges,
                honors_list,
            })
        })
    };

    view! {
        <Show when=move || viewing.get().is_some()>
            {move || match data() {
                None => view! { <div></div> }.into_any(),
                Some(v) => {
                    let tab_btn = move |i: u8, label: &'static str| {
                        view! {
                            <button
                                class=move || if tab.get() == i { "pm-tab active" } else { "pm-tab" }
                                on:click=move |_| tab.set(i)
                            >{label}</button>
                        }
                    };
                    view! {
                        <div class="modal-backdrop" on:click=close>
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
                                            <span>"Salary "<b>{v.salary}</b>
                                                {(v.years > 0).then(|| format!(" / {}yr", v.years))}
                                            </span>
                                        </div>
                                    </div>
                                </div>

                                <div class="pm-tabs">
                                    {tab_btn(0, "Attributes")}
                                    {tab_btn(1, "Stats")}
                                    {tab_btn(2, "Awards")}
                                </div>

                                // ---- Attributes tab ----
                                <Show when=move || tab.get() == 0>
                                    {let groups = v.groups.clone(); let pot = v.pot; move || view! {
                                        <div class="pm-tabnote">"Potential ceiling "<b>{pot}</b></div>
                                        <div class="pm-ratings">
                                            {groups.clone().into_iter().map(|g| view! {
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
                                    }}
                                </Show>

                                // ---- Stats tab ----
                                <Show when=move || tab.get() == 1>
                                    {let live = v.live.clone(); let totals = v.totals.clone(); let career = v.career.clone();
                                     move || view! {
                                        {live.clone().map(|pairs| view! {
                                            <h4 class="pm-section">"This Season"</h4>
                                            <div class="stat-grid">
                                                {pairs.into_iter().map(|(k, val)| view! {
                                                    <div class="stat-cell"><span class="sc-val">{val}</span><span class="sc-key">{k}</span></div>
                                                }).collect_view()}
                                            </div>
                                        })}
                                        {totals.clone().map(|pairs| view! {
                                            <h4 class="pm-section">"Career Averages"</h4>
                                            <div class="stat-grid">
                                                {pairs.into_iter().map(|(k, val)| view! {
                                                    <div class="stat-cell"><span class="sc-val">{val}</span><span class="sc-key">{k}</span></div>
                                                }).collect_view()}
                                            </div>
                                        })}
                                        <h4 class="pm-section">"Season by Season"</h4>
                                        {if career.is_empty() {
                                            view! { <p class="empty">"No completed seasons yet."</p> }.into_any()
                                        } else {
                                            view! {
                                                <div class="pm-career-scroll">
                                                <table class="tbl pm-career">
                                                    <thead><tr>
                                                        <th>"Yr"</th><th>"Tm"</th><th>"Age"</th><th>"OVR"</th><th>"GP"</th><th>"MPG"</th>
                                                        <th>"PPG"</th><th>"RPG"</th><th>"APG"</th><th>"SPG"</th><th>"BPG"</th>
                                                        <th>"FG"</th><th>"3P"</th><th>"FT"</th>
                                                    </tr></thead>
                                                    <tbody>
                                                        {career.clone().into_iter().map(|r| view! {
                                                            <tr class="row">
                                                                <td>{r.season}</td><td>{r.team}</td><td>{r.age}</td>
                                                                <td><span class="ovr">{r.ovr}</span></td><td>{r.gp}</td>
                                                                <td>{format!("{:.1}", r.mpg)}</td>
                                                                <td>{format!("{:.1}", r.ppg)}</td>
                                                                <td>{format!("{:.1}", r.rpg)}</td>
                                                                <td>{format!("{:.1}", r.apg)}</td>
                                                                <td>{format!("{:.1}", r.spg)}</td>
                                                                <td>{format!("{:.1}", r.bpg)}</td>
                                                                <td>{pct(r.fg)}</td>
                                                                <td>{pct(r.tp)}</td>
                                                                <td>{pct(r.ft)}</td>
                                                            </tr>
                                                        }).collect_view()}
                                                    </tbody>
                                                </table>
                                                </div>
                                            }.into_any()
                                        }}
                                    }}
                                </Show>

                                // ---- Awards tab ----
                                <Show when=move || tab.get() == 2>
                                    {let badges = v.honors_badges.clone(); let list = v.honors_list.clone();
                                     move || if list.is_empty() {
                                        view! { <p class="empty">"No accomplishments yet."</p> }.into_any()
                                    } else {
                                        view! {
                                            <div class="pm-honors">
                                                {badges.clone().into_iter().map(|h| view! {
                                                    <span class="honor-badge">{h}</span>
                                                }).collect_view()}
                                            </div>
                                            <ul class="honor-list">
                                                {list.clone().into_iter().map(|(season, label)| view! {
                                                    <li><span class="hl-year">{season}</span>{label}</li>
                                                }).collect_view()}
                                            </ul>
                                        }.into_any()
                                    }}
                                </Show>
                            </div>
                        </div>
                    }.into_any()
                }
            }}
        </Show>
    }
}
