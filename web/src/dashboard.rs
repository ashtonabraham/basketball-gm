//! The main dashboard: sidebar navigation, season/sim controls, and the
//! content panels (standings, schedule, roster, playoffs, history, recap).

use crate::state::{AppState, Tab};
use crate::ui::{fmt_pct, ThemeToggle};
use engine::{conference_standings, Conference, Phase, PlayoffOutcome, ROUND_NAMES};
use leptos::prelude::*;

#[component]
pub fn Dashboard() -> impl IntoView {
    let state = expect_context::<AppState>();
    let tab = state.tab;

    view! {
        <div class="dash">
            <Sidebar/>
            <main class="dash-main">
                <TopBar/>
                <ResultsBar/>
                <div class="panel">
                    {move || match tab.get() {
                        Tab::Standings => view! { <StandingsPanel/> }.into_any(),
                        Tab::Schedule => view! { <SchedulePanel/> }.into_any(),
                        Tab::Roster => view! { <RosterPanel/> }.into_any(),
                        Tab::Stats => view! { <StatsPanel/> }.into_any(),
                        Tab::Trades => view! { <TradesPanel/> }.into_any(),
                        Tab::Playoffs => view! { <PlayoffsPanel/> }.into_any(),
                        Tab::History => view! { <HistoryPanel/> }.into_any(),
                    }}
                </div>
            </main>
            <Show when=move || state.league.with(|l| l.phase == Phase::Playoffs && l.playoffs_complete())>
                <ChampionPopup/>
            </Show>
            <Show when=move || state.league.with(|l| l.phase == Phase::Offseason)>
                <RecapOverlay/>
            </Show>
            <Show when=move || state.watching.get().is_some()>
                <crate::simcast::SimcastOverlay/>
            </Show>
        </div>
    }
}

#[component]
fn ChampionPopup() -> impl IntoView {
    let state = expect_context::<AppState>();

    let champ = move || state.league.with(|l| {
        l.playoffs.as_ref().and_then(|p| p.champion)
            .and_then(|id| l.teams.iter().find(|t| t.id == id))
            .map(|t| (t.full_name(), t.primary.hex().to_string(), t.secondary.hex().to_string()))
    });
    let fmvp = move || state.league.with(|l| {
        let po = l.playoffs.as_ref()?;
        let pid = po.finals_mvp?;
        let p = l.players.iter().find(|p| p.id == pid)?;
        let s = &l.finals_stats[pid as usize];
        Some((p.name.clone(), s.ppg(), s.rpg(), s.apg()))
    });

    // Continue → finalize the season (computes awards + owner message).
    let cont = move |_| state.update_league(|l| l.finish_season());

    view! {
        <div class="overlay">
            <div class="champ-popup">
                <div class="trophy">"\u{1f3c6}"</div>
                {move || champ().map(|(name, c1, c2)| view! {
                    <div class="champ-logo" style=format!("--c1:{};--c2:{}", c1, c2)></div>
                    <h2 class="champ-name">{name}</h2>
                })}
                <div class="champ-sub">"are your champions"</div>
                {move || fmvp().map(|(name, ppg, rpg, apg)| view! {
                    <div class="fmvp">
                        <div class="fmvp-label">"Finals MVP"</div>
                        <div class="fmvp-name">{name}</div>
                        <div class="fmvp-line">{format!("{:.1} pts \u{2022} {:.1} reb \u{2022} {:.1} ast", ppg, rpg, apg)}</div>
                    </div>
                })}
                <button class="btn btn-primary big" on:click=cont>"Continue \u{2192}"</button>
            </div>
        </div>
    }
}

#[component]
fn Sidebar() -> impl IntoView {
    let state = expect_context::<AppState>();
    let tab = state.tab;

    // User team identity (reactive).
    let team_name = move || {
        state.league.with(|l| {
            l.user_team_id
                .and_then(|id| l.teams.iter().find(|t| t.id == id))
                .map(|t| t.full_name())
                .unwrap_or_default()
        })
    };
    let abbrev = move || {
        state.league.with(|l| {
            l.user_team_id
                .and_then(|id| l.teams.iter().find(|t| t.id == id))
                .map(|t| t.abbrev.clone())
                .unwrap_or_default()
        })
    };
    let colors = move || {
        state.league.with(|l| {
            l.user_team_id
                .and_then(|id| l.teams.iter().find(|t| t.id == id))
                .map(|t| (t.primary.hex().to_string(), t.secondary.hex().to_string()))
                .unwrap_or_default()
        })
    };

    let nav_btn = move |t: Tab, label: &'static str| {
        view! {
            <button
                class=move || if tab.get() == t { "nav-item active" } else { "nav-item" }
                on:click=move |_| tab.set(t)
            >
                {label}
            </button>
        }
    };

    view! {
        <aside class="sidebar">
            <div
                class="sidebar-team"
                style=move || { let (c1, c2) = colors(); format!("--c1:{};--c2:{}", c1, c2) }
            >
                <div class="sidebar-jersey"><span>{abbrev}</span></div>
                <div class="sidebar-name">{team_name}</div>
            </div>
            <nav class="nav">
                {nav_btn(Tab::Standings, "Standings")}
                {nav_btn(Tab::Schedule, "Schedule")}
                {nav_btn(Tab::Roster, "Roster")}
                {nav_btn(Tab::Stats, "Stats")}
                {nav_btn(Tab::Trades, "Trades")}
                {nav_btn(Tab::Playoffs, "Playoffs")}
                {nav_btn(Tab::History, "History")}
            </nav>
            <div class="sidebar-foot">
                <ThemeToggle/>
            </div>
        </aside>
    }
}

#[component]
fn TopBar() -> impl IntoView {
    let state = expect_context::<AppState>();

    // Reactive status pieces.
    let phase = move || state.league.with(|l| l.phase);
    let rs_done = move || state.league.with(|l| l.regular_season_complete());
    let record = move || {
        state.league.with(|l| {
            l.user_team_id
                .and_then(|id| l.teams.iter().find(|t| t.id == id))
                .map(|t| (t.wins, t.losses, t.games_played()))
                .unwrap_or((0, 0, 0))
        })
    };

    let status = move || {
        let (_, _, gp) = record();
        match phase() {
            Phase::TeamSelect => "Team Builder".to_string(),
            Phase::RegularSeason if !rs_done() => format!("Regular Season \u{2022} Game {} of 82", gp + 1),
            Phase::RegularSeason => "Regular Season Complete".to_string(),
            Phase::Playoffs => "Playoffs".to_string(),
            Phase::Offseason => "Offseason".to_string(),
            Phase::Draft => "Draft".to_string(),
            Phase::FreeAgency => "Free Agency".to_string(),
        }
    };

    // Sim handlers.
    let sim_day = move |_| state.update_league(|l| { l.sim_day(); });
    let sim_week = move |_| state.update_league(|l| l.sim_days(7));
    let sim_rest = move |_| state.update_league(|l| l.sim_to_end_of_season());
    let start_po = {
        let tab = state.tab;
        move |_| {
            state.update_league(|l| l.start_playoffs());
            tab.set(Tab::Playoffs);
        }
    };
    let view_recap = move |_| state.update_league(|l| l.finish_season());

    // Playoff stepping.
    let po_complete = move || state.league.with(|l| l.playoffs_complete());
    let sim_game = move |_| state.update_league(|l| { l.playoff_sim_gameday(); });
    let sim_round = move |_| state.update_league(|l| l.playoff_sim_round());
    let sim_po_all = move |_| state.update_league(|l| l.playoff_sim_all());

    view! {
        <div class="topbar">
            <div class="topbar-status">
                <span class="status-main">{status}</span>
                <span class="status-record">
                    {move || { let (w, l, _) = record(); format!("{}\u{2013}{}", w, l) }}
                </span>
            </div>
            <div class="topbar-actions">
                {move || match (phase(), rs_done()) {
                    (Phase::RegularSeason, false) => view! {
                        <button class="btn" on:click=sim_day>"Sim Day"</button>
                        <button class="btn" on:click=sim_week>"Sim Week"</button>
                        <button class="btn btn-primary" on:click=sim_rest>"Sim to Playoffs"</button>
                    }.into_any(),
                    (Phase::RegularSeason, true) => view! {
                        <button class="btn btn-primary" on:click=start_po>"Start Playoffs \u{2192}"</button>
                    }.into_any(),
                    (Phase::Playoffs, _) => view! {
                        {move || if po_complete() {
                            view! {
                                <button class="btn btn-primary" on:click=view_recap>"Season Recap \u{2192}"</button>
                            }.into_any()
                        } else {
                            view! {
                                <button class="btn btn-primary" on:click=sim_game>"Sim Game"</button>
                                <button class="btn" on:click=sim_round>"Sim Round"</button>
                                <button class="btn" on:click=sim_po_all>"Sim Playoffs"</button>
                            }.into_any()
                        }}
                    }.into_any(),
                    _ => view! { <span></span> }.into_any(),
                }}
            </div>
        </div>
    }
}

// ---------- Panels ----------

#[component]
fn StandingsPanel() -> impl IntoView {
    view! {
        <div class="two-col">
            <ConferenceTable conf=Conference::East title="Eastern Conference"/>
            <ConferenceTable conf=Conference::West title="Western Conference"/>
        </div>
    }
}

#[component]
fn ConferenceTable(conf: Conference, title: &'static str) -> impl IntoView {
    let state = expect_context::<AppState>();
    let rows = move || {
        state.league.with(|l| {
            let user = l.user_team_id;
            conference_standings(&l.teams, conf)
                .into_iter()
                .map(|r| {
                    let t = l.teams.iter().find(|t| t.id == r.team_id).unwrap();
                    (r.seed, t.full_name(), t.abbrev.clone(), r.wins, r.losses, r.win_pct, Some(r.team_id) == user)
                })
                .collect::<Vec<_>>()
        })
    };

    view! {
        <div class="card">
            <h3 class="card-title">{title}</h3>
            <table class="tbl">
                <thead>
                    <tr><th>"#"</th><th class="left">"Team"</th><th>"W"</th><th>"L"</th><th>"PCT"</th></tr>
                </thead>
                <tbody>
                    {move || rows().into_iter().map(|(seed, name, _ab, w, l, pct, is_user)| {
                        let cls = if is_user { "row user" } else if seed <= 8 { "row playoff" } else { "row" };
                        view! {
                            <tr class=cls>
                                <td>{seed}</td>
                                <td class="left">{name}</td>
                                <td>{w}</td>
                                <td>{l}</td>
                                <td>{fmt_pct(pct)}</td>
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
            <p class="hint">"Top 8 make the playoffs."</p>
        </div>
    }
}

#[component]
fn SchedulePanel() -> impl IntoView {
    let state = expect_context::<AppState>();
    let games = move || {
        state.league.with(|l| {
            let Some(id) = l.user_team_id else { return Vec::new() };
            let mut v: Vec<(usize, &engine::Game)> = l.schedule.iter().enumerate()
                .filter(|(_, g)| g.home == id || g.away == id).collect();
            v.sort_by_key(|(_, g)| g.day);
            v.into_iter().map(|(idx, g)| {
                let home = g.home == id;
                let opp_id = if home { g.away } else { g.home };
                let opp = l.teams.iter().find(|t| t.id == opp_id).map(|t| t.abbrev.clone()).unwrap_or_default();
                let result = g.result.map(|r| {
                    let (us, them) = if home { (r.home_score, r.away_score) } else { (r.away_score, r.home_score) };
                    (us > them, us, them)
                });
                (idx, g.day + 1, home, opp, result)
            }).collect::<Vec<_>>()
        })
    };
    let watch = move |idx: usize| state.watching.set(Some(idx));

    view! {
        <TodaysSlate/>
        <div class="card" style="margin-top:1.25rem">
            <h3 class="card-title">"Your Schedule"</h3>
            <table class="tbl">
                <thead><tr><th>"Day"</th><th class="left">"Opponent"</th><th class="left">"Result"</th><th></th></tr></thead>
                <tbody>
                    {move || games().into_iter().map(move |(idx, day, home, opp, result)| {
                        let loc = if home { "vs" } else { "@" };
                        let (cls, txt) = match result {
                            Some((win, us, them)) => (
                                if win { "row win" } else { "row loss" },
                                format!("{} {}\u{2013}{}", if win { "W" } else { "L" }, us, them),
                            ),
                            None => ("row", "\u{2014}".to_string()),
                        };
                        let played = result.is_some();
                        view! {
                            <tr class=cls>
                                <td>{day}</td>
                                <td class="left">{loc}" "{opp}</td>
                                <td class="left">{txt}</td>
                                <td>{(!played).then(|| view! { <button class="mini-btn" on:click=move |_| watch(idx)>"Watch"</button> })}</td>
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
        </div>
    }
}

/// The full slate of games on the next unplayed day — watch any of them.
#[component]
fn TodaysSlate() -> impl IntoView {
    let state = expect_context::<AppState>();
    let slate = move || state.league.with(|l| {
        let Some(day) = l.current_day() else { return (0u32, Vec::new()) };
        let games: Vec<(usize, String, String)> = l.schedule.iter().enumerate()
            .filter(|(_, g)| g.day == day && !g.is_played())
            .map(|(i, g)| {
                let ab = |id: u32| l.teams.iter().find(|t| t.id == id).map(|t| t.abbrev.clone()).unwrap_or_default();
                (i, ab(g.away), ab(g.home))
            }).collect();
        (day + 1, games)
    });
    let watch = move |idx: usize| state.watching.set(Some(idx));

    view! {
        <Show when=move || !slate().1.is_empty() fallback=|| view! { <span></span> }>
            <div class="card">
                <h3 class="card-title">{move || format!("Around the League \u{2014} Day {}", slate().0)}</h3>
                <div class="slate-grid">
                    {move || slate().1.into_iter().map(move |(idx, away, home)| view! {
                        <button class="slate-game" on:click=move |_| watch(idx)>
                            <span class="slate-match">{away}" @ "{home}</span>
                            <span class="slate-watch">"\u{25b6} Watch"</span>
                        </button>
                    }).collect_view()}
                </div>
            </div>
        </Show>
    }
}

#[component]
fn RosterPanel() -> impl IntoView {
    let state = expect_context::<AppState>();
    let players = move || {
        state.league.with(|l| {
            let Some(id) = l.user_team_id else { return Vec::new() };
            let team = l.teams.iter().find(|t| t.id == id);
            let Some(team) = team else { return Vec::new() };
            let mut ps: Vec<_> = team.roster.iter()
                .filter_map(|pid| l.players.iter().find(|p| p.id == *pid))
                .map(|p| {
                    let r = &p.ratings;
                    (p.name.clone(), p.position.abbrev(), p.age, p.overall(), p.potential,
                     p.contract.salary_str(), p.contract.years,
                     r.layup, r.dunk, r.three, r.passing, r.ball_handling,
                     r.rebounding, r.defense, r.athleticism)
                })
                .collect();
            ps.sort_by(|a, b| b.3.cmp(&a.3));
            ps
        })
    };
    let cap = move || {
        state.league.with(|l| {
            let Some(id) = l.user_team_id else { return (0.0, 0.0, 0.0, 0usize) };
            let payroll = l.team_payroll(id) as f64 / 1000.0;
            let cap = engine::SALARY_CAP as f64 / 1000.0;
            let space = l.team_cap_space(id) as f64 / 1000.0;
            let count = l.teams.iter().find(|t| t.id == id).map(|t| t.roster.len()).unwrap_or(0);
            (payroll, cap, space, count)
        })
    };

    view! {
        <div class="card">
            <div class="roster-head">
                <h3 class="card-title">"Roster"</h3>
                {move || { let (pay, cap_, space, count) = cap(); view! {
                    <div class="cap-summary">
                        <span>{format!("{} players", count)}</span>
                        <span>"Payroll "<b>{format!("${:.1}M", pay)}</b>{format!(" / ${:.0}M cap", cap_)}</span>
                        <span class=if space < 0.0 { "cap-over" } else { "cap-room" }>
                            {format!("{} ${:.1}M", if space < 0.0 { "Over by" } else { "Room:" }, space.abs())}
                        </span>
                    </div>
                }}}
            </div>
            <table class="tbl">
                <thead><tr>
                    <th class="left">"Player"</th><th>"Pos"</th><th>"Age"</th><th>"OVR"</th>
                    <th title="Potential (peak overall)">"POT"</th>
                    <th title="Salary">"Salary"</th><th title="Years left">"Yrs"</th>
                    <th title="Layup">"Lay"</th><th title="Dunk">"Dnk"</th><th title="Three-point">"3pt"</th>
                    <th title="Passing">"Pas"</th><th title="Ball handling">"Hdl"</th>
                    <th title="Rebounding">"Reb"</th><th title="Defense">"Def"</th><th title="Athleticism">"Ath"</th>
                </tr></thead>
                <tbody>
                    {move || players().into_iter().map(|(name, pos, age, ovr, pot, salary, years, lay, dnk, three, pas, hdl, reb, def, ath)| {
                        view! {
                            <tr class="row">
                                <td class="left">{name}</td>
                                <td>{pos}</td>
                                <td>{age}</td>
                                <td><span class="ovr">{ovr}</span></td>
                                <td>{pot}</td>
                                <td>{salary}</td><td>{years}</td>
                                <td>{lay}</td><td>{dnk}</td><td>{three}</td>
                                <td>{pas}</td><td>{hdl}</td>
                                <td>{reb}</td><td>{def}</td><td>{ath}</td>
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
        </div>
    }
}

#[component]
fn TradesPanel() -> impl IntoView {
    let state = expect_context::<AppState>();
    let league = state.league;

    let can_trade = move || league.with(|l| l.can_trade());
    let other = RwSignal::new(None::<u32>);
    let give = RwSignal::new(Vec::<u32>::new());
    let get = RwSignal::new(Vec::<u32>::new());
    let msg = RwSignal::new(String::new());

    // Opponent options (all teams but the user's).
    let teams = move || league.with(|l| {
        let user = l.user_team_id;
        l.teams.iter().filter(|t| Some(t.id) != user).map(|t| (t.id, t.full_name())).collect::<Vec<_>>()
    });

    // A team's roster as (id, name, pos, ovr, salary, value).
    let roster = move |tid: Option<u32>| league.with(|l| {
        let Some(tid) = tid else { return Vec::new() };
        let Some(team) = l.teams.iter().find(|t| t.id == tid) else { return Vec::new() };
        let mut v: Vec<_> = team.roster.iter().filter_map(|pid| {
            let p = l.players.iter().find(|p| p.id == *pid)?;
            Some((p.id, p.name.clone(), p.position.abbrev(), p.overall(), p.contract.salary_str(), l.player_trade_value(p.id).round() as i64))
        }).collect();
        v.sort_by(|a, b| b.3.cmp(&a.3));
        v
    });
    let user_id = move || league.with(|l| l.user_team_id);

    let eval = move || {
        let o = other.get()?;
        let (g, r) = (give.get(), get.get());
        if g.is_empty() && r.is_empty() { return None; }
        Some(league.with(|l| l.evaluate_trade(o, &g, &r)))
    };

    let toggle_give = move |pid: u32| give.update(|v| if v.contains(&pid) { v.retain(|x| *x != pid) } else { v.push(pid) });
    let toggle_get = move |pid: u32| get.update(|v| if v.contains(&pid) { v.retain(|x| *x != pid) } else { v.push(pid) });

    let propose = move |_| {
        if let Some(o) = other.get() {
            let (g, r) = (give.get(), get.get());
            let ok = { let mut d = false; state.update_league(|l| d = l.execute_trade(o, &g, &r)); d };
            if ok {
                msg.set("\u{2705} Trade completed!".into());
                give.set(vec![]);
                get.set(vec![]);
            } else {
                msg.set("They turned it down.".into());
            }
        }
    };

    // Trade finder: shop one of your players, see deals the CPU would accept.
    let shop = RwSignal::new(None::<u32>);
    let suggestions = move || league.with(|l| {
        let Some(pid) = shop.get() else { return Vec::new() };
        l.find_trades_for(pid).into_iter().map(|s| {
            let ab = l.teams.iter().find(|t| t.id == s.other).map(|t| t.full_name()).unwrap_or_default();
            (s.other, ab, s.message, s.get)
        }).collect::<Vec<_>>()
    });
    let load = move |o: u32, g: Vec<u32>, getv: Vec<u32>| {
        other.set(Some(o));
        give.set(g);
        get.set(getv);
        msg.set("Loaded — review and propose.".into());
    };

    view! {
        <Show when=can_trade fallback=|| view! {
            <div class="card"><p class="empty">"Trades are closed right now (past the in-season deadline). Come back next offseason or earlier next season."</p></div>
        }>
            <div class="card" style="margin-bottom:1.25rem">
                <div class="roster-head">
                    <h3 class="card-title">"Trade Finder"</h3>
                    <select class="input" on:change=move |e| shop.set(event_target_value(&e).parse().ok())>
                        <option value="">"Shop a player\u{2026}"</option>
                        {move || roster(user_id()).into_iter().map(|(id, name, _pos, ovr, _sal, _val)| view! {
                            <option value=id.to_string()>{format!("{} ({})", name, ovr)}</option>
                        }).collect_view()}
                    </select>
                </div>
                {move || {
                    let sugg = suggestions();
                    if shop.get().is_none() {
                        view! { <p class="hint">"Pick one of your players to see what the league would give up for him."</p> }.into_any()
                    } else if sugg.is_empty() {
                        view! { <p class="empty">"No team bit on that one. Try a more valuable player."</p> }.into_any()
                    } else {
                        view! {
                            <div class="finder-list">
                                {sugg.into_iter().map(move |(o, team, who, getv)| {
                                    let shop_pid = shop.get().unwrap();
                                    let getv2 = getv.clone();
                                    view! {
                                        <div class="finder-row">
                                            <span class="finder-team">{team}</span>
                                            <span class="finder-get">"gives "{who}</span>
                                            <button class="mini-btn draft" on:click=move |_| load(o, vec![shop_pid], getv2.clone())>"Load"</button>
                                        </div>
                                    }
                                }).collect_view()}
                            </div>
                        }.into_any()
                    }
                }}
            </div>

            <div class="card">
                <div class="roster-head">
                    <h3 class="card-title">"Trade"</h3>
                    <select class="input" on:change=move |e| {
                        other.set(event_target_value(&e).parse().ok());
                        get.set(vec![]);
                        msg.set(String::new());
                    }>
                        <option value="">"Select a team\u{2026}"</option>
                        {move || teams().into_iter().map(|(id, name)| view! {
                            <option value=id.to_string()>{name}</option>
                        }).collect_view()}
                    </select>
                </div>

                <div class="trade-cols">
                    <div class="trade-side">
                        <h4 class="round-name">"You send"</h4>
                        {move || roster(user_id()).into_iter().map(move |(id, name, pos, ovr, sal, val)| {
                            let picked = move || give.get().contains(&id);
                            view! {
                                <div class=move || if picked() { "trade-row picked" } else { "trade-row" }
                                    on:click=move |_| toggle_give(id)>
                                    <span class="tr-name">{name}</span>
                                    <span class="tr-meta">{pos}" \u{2022} "{ovr}" \u{2022} "{sal}" \u{2022} val "{val}</span>
                                </div>
                            }
                        }).collect_view()}
                    </div>
                    <div class="trade-side">
                        <h4 class="round-name">"You receive"</h4>
                        {move || roster(other.get()).into_iter().map(move |(id, name, pos, ovr, sal, val)| {
                            let picked = move || get.get().contains(&id);
                            view! {
                                <div class=move || if picked() { "trade-row picked" } else { "trade-row" }
                                    on:click=move |_| toggle_get(id)>
                                    <span class="tr-name">{name}</span>
                                    <span class="tr-meta">{pos}" \u{2022} "{ovr}" \u{2022} "{sal}" \u{2022} val "{val}</span>
                                </div>
                            }
                        }).collect_view()}
                    </div>
                </div>

                {move || eval().map(|e| {
                    let cls = if e.accepted { "trade-verdict ok" } else if e.legal { "trade-verdict warn" } else { "trade-verdict bad" };
                    view! {
                        <div class=cls>
                            <div class="trade-salaries">
                                {format!("Out ${:.1}M  \u{2194}  In ${:.1}M", e.give_salary as f64 / 1000.0, e.get_salary as f64 / 1000.0)}
                            </div>
                            <div class="trade-msg">{e.message}</div>
                        </div>
                    }
                })}

                <div class="offer-actions">
                    <button class="btn btn-primary" on:click=propose
                        disabled=move || !eval().map(|e| e.accepted).unwrap_or(false)>
                        "Propose Trade"
                    </button>
                    <span class="offer-msg">{move || msg.get()}</span>
                </div>
            </div>
        </Show>
    }
}

/// One series rendered in the bracket.
#[derive(Clone)]
struct SeriesView {
    ha: String, hc1: String, hc2: String, hw: u8,
    la: String, lc1: String, lc2: String, lw: u8,
    hi_won: bool, lo_won: bool, has_user: bool,
}

#[component]
fn SeriesBox(data: Option<SeriesView>) -> impl IntoView {
    match data {
        None => view! { <div class="series placeholder"><div class="seed-line">"\u{2014}"</div><div class="seed-line">"\u{2014}"</div></div> }.into_any(),
        Some(s) => {
            let cls = if s.has_user { "series user" } else { "series" };
            view! {
                <div class=cls>
                    <div class=if s.hi_won { "seed-line won" } else { "seed-line" }>
                        <span class="mini-logo" style=format!("--c1:{};--c2:{}", s.hc1, s.hc2)>{s.ha.clone()}</span>
                        <span class="seed-name">{s.ha}</span>
                        <span class="wins">{s.hw}</span>
                    </div>
                    <div class=if s.lo_won { "seed-line won" } else { "seed-line" }>
                        <span class="mini-logo" style=format!("--c1:{};--c2:{}", s.lc1, s.lc2)>{s.la.clone()}</span>
                        <span class="seed-name">{s.la}</span>
                        <span class="wins">{s.lw}</span>
                    </div>
                </div>
            }.into_any()
        }
    }
}

#[component]
fn PlayoffsPanel() -> impl IntoView {
    let state = expect_context::<AppState>();
    let has_po = move || state.league.with(|l| l.playoffs.is_some());

    // Build the seven columns of a 2K-style bracket: West fans in from the
    // left, East from the right, Finals in the middle. Each column is padded
    // with `None` placeholders so the bracket shape shows before it fills in.
    let bracket = move || {
        state.league.with(|l| {
            let uid = l.user_team_id;
            let badge = |id: u32| {
                l.teams.iter().find(|t| t.id == id)
                    .map(|t| (t.abbrev.clone(), t.primary.hex().to_string(), t.secondary.hex().to_string()))
                    .unwrap_or_default()
            };
            let mk = |s: &engine::Series| {
                let w = s.winner();
                let (ha, hc1, hc2) = badge(s.high);
                let (la, lc1, lc2) = badge(s.low);
                SeriesView {
                    ha, hc1, hc2, hw: s.high_wins, la, lc1, lc2, lw: s.low_wins,
                    hi_won: w == Some(s.high), lo_won: w == Some(s.low),
                    has_user: uid == Some(s.high) || uid == Some(s.low),
                }
            };
            let po = l.playoffs.as_ref();
            // East = first half of each round's series; West = second half.
            let half = |r: usize, west: bool, count: usize| -> Vec<Option<SeriesView>> {
                let mut v = Vec::new();
                if let Some(po) = po {
                    if let Some(round) = po.rounds.get(r) {
                        let (start, end) = if r == 3 {
                            (0, round.len())
                        } else if west {
                            (round.len() / 2, round.len())
                        } else {
                            (0, round.len() / 2)
                        };
                        for s in &round[start..end] {
                            v.push(Some(mk(s)));
                        }
                    }
                }
                while v.len() < count { v.push(None); }
                v
            };
            let champ = po.and_then(|p| p.champion)
                .and_then(|id| l.teams.iter().find(|t| t.id == id))
                .map(|t| t.full_name());
            (
                half(0, true, 4), half(1, true, 2), half(2, true, 1),
                half(3, false, 1),
                half(2, false, 1), half(1, false, 2), half(0, false, 4),
                champ,
            )
        })
    };

    view! {
        <Show
            when=has_po
            fallback=|| view! { <div class="card"><p class="empty">"The playoffs haven\u{2019}t started yet. Finish the regular season first."</p></div> }
        >
            <div class="card">
                <h3 class="card-title">"Playoff Bracket"</h3>
                {move || {
                    let (w_r1, w_se, w_cf, finals, e_cf, e_se, e_r1, champ) = bracket();
                    let col = |title: &str, slots: Vec<Option<SeriesView>>| {
                        let t = title.to_string();
                        view! {
                            <div class="bracket-col">
                                <h4 class="round-name">{t}</h4>
                                {slots.into_iter().map(|s| view! { <SeriesBox data=s/> }).collect_view()}
                            </div>
                        }
                    };
                    view! {
                        <div class="bracket bracket-2k">
                            <div class="conf-side">
                                <div class="conf-label west">"Western"</div>
                                <div class="conf-cols">
                                    {col("First Round", w_r1)}
                                    {col("Semis", w_se)}
                                    {col("Conf Finals", w_cf)}
                                </div>
                            </div>
                            <div class="bracket-center">
                                <h4 class="round-name">"Finals"</h4>
                                <SeriesBox data=finals.into_iter().next().flatten()/>
                                {champ.map(|c| view! { <div class="champ-banner center">"\u{1f3c6} "{c}</div> })}
                            </div>
                            <div class="conf-side">
                                <div class="conf-label east">"Eastern"</div>
                                <div class="conf-cols">
                                    {col("Conf Finals", e_cf)}
                                    {col("Semis", e_se)}
                                    {col("First Round", e_r1)}
                                </div>
                            </div>
                        </div>
                    }
                }}
            </div>
        </Show>
    }
}

#[component]
fn HistoryPanel() -> impl IntoView {
    let state = expect_context::<AppState>();
    let rows = move || state.league.with(|l| {
        l.history.iter().rev().map(|h| {
            let outcome = describe_outcome(h.user_outcome);
            (h.season, format!("{}\u{2013}{}", h.user_wins, h.user_losses), outcome, h.champion_name.clone())
        }).collect::<Vec<_>>()
    });

    view! {
        <div class="card">
            <h3 class="card-title">"Franchise History"</h3>
            <Show
                when=move || !rows().is_empty()
                fallback=|| view! { <p class="empty">"No completed seasons yet."</p> }
            >
                <table class="tbl">
                    <thead><tr><th>"Season"</th><th>"Record"</th><th class="left">"Result"</th><th class="left">"Champion"</th></tr></thead>
                    <tbody>
                        {move || rows().into_iter().map(|(season, rec, outcome, champ)| view! {
                            <tr class="row">
                                <td>{season}</td>
                                <td>{rec}</td>
                                <td class="left">{outcome}</td>
                                <td class="left">{champ}</td>
                            </tr>
                        }).collect_view()}
                    </tbody>
                </table>
            </Show>
        </div>
    }
}

#[component]
fn ResultsBar() -> impl IntoView {
    let state = expect_context::<AppState>();
    let games = move || {
        state.league.with(|l| {
            let Some(id) = l.user_team_id else { return Vec::new() };
            let mut v: Vec<_> = l
                .schedule
                .iter()
                .filter(|g| (g.home == id || g.away == id) && g.is_played())
                .collect();
            v.sort_by_key(|g| g.day);
            v.into_iter()
                .filter_map(|g| {
                    let home = g.home == id;
                    let opp_id = if home { g.away } else { g.home };
                    let opp = l.teams.iter().find(|t| t.id == opp_id).map(|t| t.abbrev.clone())?;
                    let r = g.result?;
                    let (us, them) = if home { (r.home_score, r.away_score) } else { (r.away_score, r.home_score) };
                    let win = us > them;
                    let loc = if home { "vs" } else { "@" };
                    Some((win, loc, opp, us, them))
                })
                .collect::<Vec<_>>()
        })
    };

    view! {
        <Show when=move || !games().is_empty()>
            <div class="results-bar">
                {move || games().into_iter().map(|(win, loc, opp, us, them)| {
                    let tip = format!("{} {}\u{2013}{} {} {}", if win { "W" } else { "L" }, us, them, loc, opp);
                    view! {
                        <span class=if win { "result-chip w" } else { "result-chip l" } title=tip>
                            <span class="rc-top">{if win { "W" } else { "L" }}" "{loc}" "{opp.clone()}</span>
                            <span class="rc-score">{format!("{}\u{2013}{}", us, them)}</span>
                        </span>
                    }
                }).collect_view()}
            </div>
        </Show>
    }
}

#[component]
fn StatsPanel() -> impl IntoView {
    let state = expect_context::<AppState>();

    // User team per-game stats.
    let team_rows = move || {
        state.league.with(|l| {
            let Some(id) = l.user_team_id else { return Vec::new() };
            let Some(team) = l.teams.iter().find(|t| t.id == id) else { return Vec::new() };
            let mut rows: Vec<_> = team
                .roster
                .iter()
                .filter_map(|pid| l.players.iter().find(|p| p.id == *pid))
                .map(|p| {
                    let s = &l.season_stats[p.id as usize];
                    (p.name.clone(), p.position.abbrev(), s.gp, s.mpg(), s.ppg(), s.rpg(), s.apg(), s.fg_pct(), s.tp_pct())
                })
                .collect();
            rows.sort_by(|a, b| b.4.partial_cmp(&a.4).unwrap());
            rows
        })
    };

    // League scoring leaders.
    let leaders = move || {
        state.league.with(|l| {
            let mut rows: Vec<_> = l
                .players
                .iter()
                .filter(|p| l.season_stats[p.id as usize].gp > 0)
                .map(|p| {
                    let s = &l.season_stats[p.id as usize];
                    let team = p.team.and_then(|tid| l.teams.iter().find(|t| t.id == tid)).map(|t| t.abbrev.clone()).unwrap_or_default();
                    (p.name.clone(), team, s.ppg())
                })
                .collect();
            rows.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap());
            rows.truncate(10);
            rows
        })
    };

    let any_games = move || state.league.with(|l| l.season_stats.iter().any(|s| s.gp > 0));

    view! {
        <Show
            when=any_games
            fallback=|| view! { <div class="card"><p class="empty">"Play some games to see stats."</p></div> }
        >
            <div class="two-col">
                <div class="card">
                    <h3 class="card-title">"Your Team \u{2014} Per Game"</h3>
                    <table class="tbl">
                        <thead><tr>
                            <th class="left">"Player"</th><th>"Pos"</th><th>"GP"</th><th>"MPG"</th>
                            <th>"PPG"</th><th>"RPG"</th><th>"APG"</th><th>"FG%"</th><th>"3P%"</th>
                        </tr></thead>
                        <tbody>
                            {move || team_rows().into_iter().map(|(name, pos, gp, mpg, ppg, rpg, apg, fg, tp)| view! {
                                <tr class="row">
                                    <td class="left">{name}</td><td>{pos}</td><td>{gp}</td>
                                    <td>{format!("{:.1}", mpg)}</td>
                                    <td><b>{format!("{:.1}", ppg)}</b></td>
                                    <td>{format!("{:.1}", rpg)}</td>
                                    <td>{format!("{:.1}", apg)}</td>
                                    <td>{fmt_pct(fg)}</td>
                                    <td>{fmt_pct(tp)}</td>
                                </tr>
                            }).collect_view()}
                        </tbody>
                    </table>
                </div>
                <div class="card">
                    <h3 class="card-title">"League Scoring Leaders"</h3>
                    <table class="tbl">
                        <thead><tr><th>"#"</th><th class="left">"Player"</th><th>"Team"</th><th>"PPG"</th></tr></thead>
                        <tbody>
                            {move || leaders().into_iter().enumerate().map(|(i, (name, team, ppg))| view! {
                                <tr class="row">
                                    <td>{i + 1}</td>
                                    <td class="left">{name}</td>
                                    <td>{team}</td>
                                    <td><b>{format!("{:.1}", ppg)}</b></td>
                                </tr>
                            }).collect_view()}
                        </tbody>
                    </table>
                </div>
            </div>
        </Show>
    }
}

fn describe_outcome(o: PlayoffOutcome) -> String {
    match o {
        PlayoffOutcome::MissedPlayoffs => "Missed playoffs".to_string(),
        PlayoffOutcome::WonChampionship => "\u{1f3c6} Champions".to_string(),
        PlayoffOutcome::LostInRound(r) => format!("Lost in {}", ROUND_NAMES.get(r).copied().unwrap_or("playoffs")),
    }
}

#[component]
fn RecapOverlay() -> impl IntoView {
    let state = expect_context::<AppState>();
    let recap = move || state.league.with(|l| l.season_recap());

    // Owner's note, styled by tone.
    let owner = move || state.league.with(|l| l.owner_message.clone());
    let owner_class = move || {
        state.league.with(|l| match l.owner_message.as_ref().map(|m| m.tone) {
            Some(engine::OwnerTone::Pleased) => "owner pleased",
            Some(engine::OwnerTone::Displeased) => "owner displeased",
            Some(engine::OwnerTone::TooEarly) => "owner early",
            _ => "owner",
        })
    };

    // Resolve award winners to names.
    let award_name = move |pick: fn(&engine::Awards) -> Option<u32>| {
        state.league.with(|l| {
            l.awards.as_ref().and_then(pick).and_then(|id| {
                l.players.iter().find(|p| p.id == id).map(|p| format!("{} ({} OVR)", p.name, p.overall()))
            })
        })
    };

    let to_draft = move |_| state.update_league(|l| l.enter_draft());

    view! {
        <div class="overlay">
            <div class="recap-card">
                {move || recap().map(|r| {
                    let outcome = describe_outcome(r.outcome);
                    let seed = r.conference_seed.map(|s| format!("#{} seed", s)).unwrap_or_else(|| "Missed playoffs".into());
                    view! {
                        <h2 class="recap-title">"Season "{r.season}" Recap"</h2>
                        <div class="recap-team">{r.team_name}</div>
                        <div class="recap-stats">
                            <div class="stat"><div class="stat-num">{format!("{}\u{2013}{}", r.wins, r.losses)}</div><div class="stat-lbl">"Record"</div></div>
                            <div class="stat"><div class="stat-num">{seed}</div><div class="stat-lbl">"Conference"</div></div>
                            <div class="stat"><div class="stat-num">{outcome}</div><div class="stat-lbl">"Postseason"</div></div>
                        </div>
                    }
                })}

                // Message from the owner.
                {move || owner().map(|m| view! {
                    <div class=owner_class()>
                        <div class="owner-label">"\u{1f4e9} A message from the owner"</div>
                        <div class="owner-body">"\u{201c}"{m.body}"\u{201d}"</div>
                    </div>
                })}

                // Awards.
                <div class="awards">
                    <div class="award"><span class="award-lbl">"MVP"</span><span class="award-name">{move || award_name(|a| a.mvp).unwrap_or_else(|| "\u{2014}".into())}</span></div>
                    <div class="award"><span class="award-lbl">"Defensive POY"</span><span class="award-name">{move || award_name(|a| a.dpoy).unwrap_or_else(|| "\u{2014}".into())}</span></div>
                    <div class="award"><span class="award-lbl">"Rookie of the Year"</span><span class="award-name">{move || award_name(|a| a.roy).unwrap_or_else(|| "\u{2014}".into())}</span></div>
                </div>

                {move || recap().map(|r| view! {
                    <div class="recap-line">"League Champion: "<b>{r.champion_name}</b></div>
                })}
                <button class="btn btn-primary big" on:click=to_draft>"Continue to Draft \u{2192}"</button>
            </div>
        </div>
    }
}
