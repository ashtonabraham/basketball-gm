//! Player detail modal: opens when a player's name is clicked anywhere in the
//! app. Three tabs — Attributes (bar-graph ratings), Stats (2K-style career
//! grid: newest season on top, career averages at the bottom), and Awards.

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

/// One row of the career stats grid (a season, or the career-average total).
#[derive(Clone)]
struct StatRow {
    season: String,
    team: String,
    age: String,
    ovr: String,
    gp: u32,
    mpg: f64,
    ppg: f64,
    rpg: f64,
    apg: f64,
    spg: f64,
    bpg: f64,
    tov: f64,
    fg: f64,
    tp: f64,
    ft: f64,
    total: bool,
}

impl StatRow {
    fn from_season(season: u32, team: String, age: u8, ovr: u8, s: &SeasonStats) -> StatRow {
        StatRow {
            season: season.to_string(),
            team,
            age: age.to_string(),
            ovr: ovr.to_string(),
            gp: s.gp,
            mpg: s.mpg(),
            ppg: s.ppg(),
            rpg: s.rpg(),
            apg: s.apg(),
            spg: s.spg(),
            bpg: s.bpg(),
            tov: s.tovpg(),
            fg: s.fg_pct(),
            tp: s.tp_pct(),
            ft: s.ft_pct(),
            total: false,
        }
    }
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
    trait_label: &'static str,
    trait_blurb: &'static str,
    morale: f64,
    wants_trade: bool,
    groups: Vec<Group>,
    stats: Vec<StatRow>,
    honors_badges: Vec<String>,
    honors_list: Vec<(u32, &'static str)>,
}

fn pct(x: f64) -> String {
    format!("{:.1}%", x * 100.0)
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
            let team_abbrev = p
                .team
                .and_then(|t| l.teams.iter().find(|tm| tm.id == t))
                .map(|t| t.abbrev.clone())
                .unwrap_or_default();

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

            // Build the career grid: current (in-progress) season on top, then
            // logged seasons newest-first, then a career-average total row.
            let mut stats: Vec<StatRow> = Vec::new();
            let mut total = SeasonStats::default();

            let st = &l.season_stats[pid as usize];
            let logged_this = career.map(|c| c.seasons.last().map(|cs| cs.season == l.season).unwrap_or(false)).unwrap_or(false);
            if st.gp > 0 && !logged_this {
                stats.push(StatRow::from_season(l.season, team_abbrev.clone(), p.age, p.overall(), st));
                total.add(st);
            }
            if let Some(c) = career {
                for cs in c.seasons.iter().rev() {
                    stats.push(StatRow::from_season(cs.season, cs.team_abbrev.clone(), cs.age, cs.overall, &cs.stats));
                    total.add(&cs.stats);
                }
            }
            if total.gp > 0 && stats.len() > 1 {
                let mut trow = StatRow::from_season(0, String::new(), 0, 0, &total);
                trow.season = "Career".into();
                trow.age = String::new();
                trow.ovr = String::new();
                trow.total = true;
                stats.push(trow);
            }

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
                trait_label: p.personality.label(),
                trait_blurb: p.personality.blurb(),
                morale: p.morale,
                wants_trade: p.team.is_some() && p.morale < 0.30,
                groups,
                stats,
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

                                <div class="pm-persona">
                                    <span class="trait-chip" title=v.trait_blurb>{v.trait_label}</span>
                                    <span class="morale-wrap">
                                        <span class="morale-label">"Morale"</span>
                                        <span class="morale-bar">
                                            <span class=move || {
                                                let lvl = if v.morale >= 0.6 { "morale-fill hi" } else if v.morale >= 0.35 { "morale-fill mid" } else { "morale-fill lo" };
                                                lvl
                                            } style=format!("width:{}%", (v.morale * 100.0).round())></span>
                                        </span>
                                        <span class="morale-pct">{format!("{:.0}%", v.morale * 100.0)}</span>
                                    </span>
                                    {v.wants_trade.then(|| view! { <span class="trade-flag">"\u{26a0} Wants a trade"</span> })}
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

                                // ---- Stats tab (2K-style career grid) ----
                                <Show when=move || tab.get() == 1>
                                    {let stats = v.stats.clone(); move || if stats.is_empty() {
                                        view! { <p class="empty">"No games played yet."</p> }.into_any()
                                    } else {
                                        view! {
                                            <h4 class="pm-section">"Career Stats"</h4>
                                            <div class="pm-career-scroll">
                                                <table class="tbl pm-career">
                                                    <thead><tr>
                                                        <th>"Season"</th><th>"Tm"</th><th>"Age"</th><th>"OVR"</th><th>"GP"</th><th>"MPG"</th>
                                                        <th>"PPG"</th><th>"RPG"</th><th>"APG"</th><th>"SPG"</th><th>"BPG"</th><th>"TOV"</th>
                                                        <th>"FG%"</th><th>"3P%"</th><th>"FT%"</th>
                                                    </tr></thead>
                                                    <tbody>
                                                        {stats.clone().into_iter().map(|r| {
                                                            let cls = if r.total { "row pm-totals" } else { "row" };
                                                            view! {
                                                                <tr class=cls>
                                                                    <td class="left">{r.season}</td><td>{r.team}</td><td>{r.age}</td>
                                                                    <td>{r.ovr}</td><td>{r.gp}</td>
                                                                    <td>{format!("{:.1}", r.mpg)}</td>
                                                                    <td>{format!("{:.1}", r.ppg)}</td>
                                                                    <td>{format!("{:.1}", r.rpg)}</td>
                                                                    <td>{format!("{:.1}", r.apg)}</td>
                                                                    <td>{format!("{:.1}", r.spg)}</td>
                                                                    <td>{format!("{:.1}", r.bpg)}</td>
                                                                    <td>{format!("{:.1}", r.tov)}</td>
                                                                    <td>{pct(r.fg)}</td>
                                                                    <td>{pct(r.tp)}</td>
                                                                    <td>{pct(r.ft)}</td>
                                                                </tr>
                                                            }
                                                        }).collect_view()}
                                                    </tbody>
                                                </table>
                                            </div>
                                        }.into_any()
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
