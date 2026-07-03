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
                        Tab::Finances => view! { <FinancesPanel/> }.into_any(),
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
                {nav_btn(Tab::Finances, "Finances")}
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

/// Sim a scheduled game once (applying its result + stats) and open the simcast
/// on the resulting play-by-play. Doing the sim here — not inside the overlay —
/// guarantees it runs exactly once, so the overlay actually stays open.
fn watch_game(state: AppState, idx: usize) {
    let mut ev = Vec::new();
    state.update_league(|l| ev = l.watch_scheduled_game(idx).unwrap_or_default());
    if ev.is_empty() {
        return;
    }
    state.watch_events.set_value(ev);
    state.watching.set(Some(idx));
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
    let watch = move |idx: usize| watch_game(state, idx);

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
    let watch = move |idx: usize| watch_game(state, idx);

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
                    (p.id, p.name.clone(), p.position.abbrev(), p.age, p.overall(), p.potential,
                     p.contract.salary_str(), p.contract.years,
                     r.inside(), r.outside(), r.playmaking(), r.defending(), r.athletic())
                })
                .collect();
            ps.sort_by(|a, b| b.4.cmp(&a.4));
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
                    <th title="Inside scoring">"INS"</th><th title="Outside shooting">"OUT"</th>
                    <th title="Playmaking">"PMK"</th><th title="Defense">"DEF"</th><th title="Physical">"ATH"</th>
                </tr></thead>
                <tbody>
                    {move || players().into_iter().map(|(id, name, pos, age, ovr, pot, salary, years, ins, out, plm, def, ath)| {
                        view! {
                            <tr class="row">
                                <td class="left"><crate::ui::PlayerLink id=id name=name/></td>
                                <td>{pos}</td>
                                <td>{age}</td>
                                <td><span class="ovr">{ovr}</span></td>
                                <td>{pot}</td>
                                <td>{salary}</td><td>{years}</td>
                                <td>{ins}</td><td>{out}</td><td>{plm}</td><td>{def}</td><td>{ath}</td>
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
        </div>
    }
}

/// A trade package resolved to display strings + raw ids for execution.
#[derive(Clone)]
struct PkgView {
    other: u32,
    give: Vec<String>,
    get: Vec<String>,
    give_value: i64,
    get_value: i64,
    give_players: Vec<u32>,
    give_picks: Vec<u32>,
    get_players: Vec<u32>,
    get_picks: Vec<u32>,
}

fn pkgview(l: &engine::League, p: &engine::TradePackage) -> PkgView {
    let pname = |id: u32| l.players.iter().find(|x| x.id == id).map(|x| format!("{} ({})", x.name, x.overall())).unwrap_or_default();
    let give: Vec<String> = p.give_players.iter().map(|id| pname(*id))
        .chain(p.give_picks.iter().map(|id| l.pick_label(*id))).collect();
    let get: Vec<String> = p.get_players.iter().map(|id| pname(*id))
        .chain(p.get_picks.iter().map(|id| l.pick_label(*id))).collect();
    PkgView {
        other: p.other, give, get,
        give_value: p.give_value.round() as i64,
        get_value: p.get_value.round() as i64,
        give_players: p.give_players.clone(), give_picks: p.give_picks.clone(),
        get_players: p.get_players.clone(), get_picks: p.get_picks.clone(),
    }
}

#[component]
fn TradesPanel() -> impl IntoView {
    let state = expect_context::<AppState>();
    let league = state.league;

    let can_trade = move || league.with(|l| l.can_trade());
    let mode = RwSignal::new(0u8); // 0 = acquire, 1 = shop, 2 = manual
    let msg = RwSignal::new(String::new());

    let user_id = move || league.with(|l| l.user_team_id);
    let teams = move || league.with(|l| {
        let user = l.user_team_id;
        l.teams.iter().filter(|t| Some(t.id) != user).map(|t| (t.id, t.full_name())).collect::<Vec<_>>()
    });
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
    let picks = move |tid: Option<u32>| league.with(|l| {
        let Some(tid) = tid else { return Vec::new() };
        l.picks_owned_by(tid).into_iter().map(|pk| (pk.id, l.pick_label(pk.id), l.pick_value(pk).round() as i64)).collect::<Vec<_>>()
    });

    // A reusable package card with a value meter and an Accept button.
    let pkg_card = move |p: PkgView| {
        let total = (p.give_value + p.get_value).max(1) as f64;
        let fill = format!("{:.0}%", p.get_value as f64 / total * 100.0);
        let p2 = p.clone();
        let (gv, rv) = (p.give_value, p.get_value);
        view! {
            <div class="pkg-card">
                <div class="pkg-cols">
                    <div class="pkg-side">
                        <div class="pkg-h">"You give"</div>
                        {p.give.into_iter().map(|s| view! { <div class="pkg-asset out">{s}</div> }).collect_view()}
                    </div>
                    <div class="pkg-side">
                        <div class="pkg-h">"You get"</div>
                        {p.get.into_iter().map(|s| view! { <div class="pkg-asset in">{s}</div> }).collect_view()}
                    </div>
                </div>
                <div class="pkg-meter" title="Value balance"><span class="pkg-fill" style=format!("width:{}", fill)></span></div>
                <div class="pkg-foot">
                    <span class="pkg-vals">{format!("give {} \u{2194} get {}", gv, rv)}</span>
                    <button class="btn btn-primary" on:click=move |_| {
                        let pk = p2.clone();
                        let ok = { let mut d = false; state.update_league(|l| d = l.execute_trade_full(pk.other, &pk.give_players, &pk.give_picks, &pk.get_players, &pk.get_picks)); d };
                        msg.set(if ok { "\u{2705} Trade completed!".into() } else { "They turned it down.".into() });
                    }>"Accept Trade"</button>
                </div>
            </div>
        }
    };

    // ---- Acquire (buy) ----
    let target_team = RwSignal::new(None::<u32>);
    let target = RwSignal::new(None::<u32>);
    let buy_pkgs = move || league.with(|l| {
        let Some(t) = target.get() else { return Vec::new() };
        l.find_packages_to_acquire(t).iter().map(|p| pkgview(l, p)).collect::<Vec<_>>()
    });

    // ---- Shop (sell) ----
    let shop = RwSignal::new(None::<u32>);
    let sell_pkgs = move || league.with(|l| {
        let Some(s) = shop.get() else { return Vec::new() };
        l.find_packages_to_trade_away(s).iter().map(|p| pkgview(l, p)).collect::<Vec<_>>()
    });

    // ---- Manual builder ----
    let other = RwSignal::new(None::<u32>);
    let gp = RwSignal::new(Vec::<u32>::new());
    let gk = RwSignal::new(Vec::<u32>::new());
    let rp = RwSignal::new(Vec::<u32>::new());
    let rk = RwSignal::new(Vec::<u32>::new());
    let eval = move || {
        let o = other.get()?;
        if gp.get().is_empty() && gk.get().is_empty() && rp.get().is_empty() && rk.get().is_empty() { return None; }
        Some(league.with(|l| l.evaluate_trade_full(o, &gp.get(), &gk.get(), &rp.get(), &rk.get())))
    };
    let propose = move |_| {
        if let Some(o) = other.get() {
            let ok = { let mut d = false; state.update_league(|l| d = l.execute_trade_full(o, &gp.get(), &gk.get(), &rp.get(), &rk.get())); d };
            if ok { msg.set("\u{2705} Trade completed!".into()); gp.set(vec![]); gk.set(vec![]); rp.set(vec![]); rk.set(vec![]); }
            else { msg.set("They turned it down.".into()); }
        }
    };

    let mode_btn = move |m: u8, label: &'static str| view! {
        <button class=move || if mode.get() == m { "seg active" } else { "seg" } on:click=move |_| { mode.set(m); msg.set(String::new()); }>{label}</button>
    };

    view! {
        <Show when=can_trade fallback=|| view! {
            <div class="card"><p class="empty">"Trades are closed right now (past the in-season deadline). Come back next offseason or earlier next season."</p></div>
        }>
            <div class="seg-row">
                {mode_btn(0, "Acquire a Player")}
                {mode_btn(1, "Shop a Player")}
                {mode_btn(2, "Build a Trade")}
            </div>

            // ===== Acquire =====
            <Show when=move || mode.get() == 0>
                <div class="card">
                    <div class="roster-head">
                        <h3 class="card-title">"Find a Trade for a Target"</h3>
                        <div class="finder-selects">
                            <select class="input" on:change=move |e| { target_team.set(event_target_value(&e).parse().ok()); target.set(None); }>
                                <option value="">"Team\u{2026}"</option>
                                {move || teams().into_iter().map(|(id, name)| view! { <option value=id.to_string()>{name}</option> }).collect_view()}
                            </select>
                            <select class="input" prop:value=move || target.get().map(|t| t.to_string()).unwrap_or_default()
                                on:change=move |e| target.set(event_target_value(&e).parse().ok())>
                                <option value="">"Player\u{2026}"</option>
                                {move || roster(target_team.get()).into_iter().map(|(id, name, _p, ovr, _s, _v)| view! {
                                    <option value=id.to_string()>{format!("{} ({})", name, ovr)}</option>
                                }).collect_view()}
                            </select>
                        </div>
                    </div>
                    {move || {
                        if target.get().is_none() {
                            view! { <p class="hint">"Pick a player you want, and the finder builds packages (players + picks) that land him."</p> }.into_any()
                        } else {
                            let pkgs = buy_pkgs();
                            if pkgs.is_empty() {
                                view! { <p class="empty">"No workable package \u{2014} you may not have enough to match, or he's not available. Try adding picks manually."</p> }.into_any()
                            } else {
                                view! { <div class="pkg-list">{pkgs.into_iter().map(pkg_card).collect_view()}</div> }.into_any()
                            }
                        }
                    }}
                </div>
            </Show>

            // ===== Shop =====
            <Show when=move || mode.get() == 1>
                <div class="card">
                    <div class="roster-head">
                        <h3 class="card-title">"Shop One of Your Players"</h3>
                        <select class="input" on:change=move |e| shop.set(event_target_value(&e).parse().ok())>
                            <option value="">"Your player\u{2026}"</option>
                            {move || roster(user_id()).into_iter().map(|(id, name, _p, ovr, _s, _v)| view! {
                                <option value=id.to_string()>{format!("{} ({})", name, ovr)}</option>
                            }).collect_view()}
                        </select>
                    </div>
                    {move || {
                        if shop.get().is_none() {
                            view! { <p class="hint">"Pick a player to shop and see the best package each team would send back."</p> }.into_any()
                        } else {
                            let pkgs = sell_pkgs();
                            if pkgs.is_empty() {
                                view! { <p class="empty">"No team bit on that one. Try a more valuable player."</p> }.into_any()
                            } else {
                                view! { <div class="pkg-list">{pkgs.into_iter().map(pkg_card).collect_view()}</div> }.into_any()
                            }
                        }
                    }}
                </div>
            </Show>

            // ===== Manual builder =====
            <Show when=move || mode.get() == 2>
                <div class="card">
                    <div class="roster-head">
                        <h3 class="card-title">"Build a Trade"</h3>
                        <select class="input" on:change=move |e| { other.set(event_target_value(&e).parse().ok()); rp.set(vec![]); rk.set(vec![]); msg.set(String::new()); }>
                            <option value="">"Select a team\u{2026}"</option>
                            {move || teams().into_iter().map(|(id, name)| view! { <option value=id.to_string()>{name}</option> }).collect_view()}
                        </select>
                    </div>
                    <div class="trade-cols">
                        <div class="trade-side">
                            <h4 class="round-name">"You send"</h4>
                            {move || roster(user_id()).into_iter().map(move |(id, name, pos, ovr, sal, val)| {
                                let picked = move || gp.get().contains(&id);
                                view! {
                                    <div class=move || if picked() { "trade-row picked" } else { "trade-row" }
                                        on:click=move |_| gp.update(|v| if v.contains(&id) { v.retain(|x| *x != id) } else { v.push(id) })>
                                        <span class="tr-name">{name}</span>
                                        <span class="tr-meta">{pos}" \u{2022} "{ovr}" \u{2022} "{sal}" \u{2022} val "{val}</span>
                                    </div>
                                }
                            }).collect_view()}
                            {move || picks(user_id()).into_iter().map(move |(id, label, val)| {
                                let picked = move || gk.get().contains(&id);
                                view! {
                                    <div class=move || if picked() { "trade-row pick picked" } else { "trade-row pick" }
                                        on:click=move |_| gk.update(|v| if v.contains(&id) { v.retain(|x| *x != id) } else { v.push(id) })>
                                        <span class="tr-name">"\u{1f4c4} "{label}</span>
                                        <span class="tr-meta">"pick \u{2022} val "{val}</span>
                                    </div>
                                }
                            }).collect_view()}
                        </div>
                        <div class="trade-side">
                            <h4 class="round-name">"You receive"</h4>
                            {move || roster(other.get()).into_iter().map(move |(id, name, pos, ovr, sal, val)| {
                                let picked = move || rp.get().contains(&id);
                                view! {
                                    <div class=move || if picked() { "trade-row picked" } else { "trade-row" }
                                        on:click=move |_| rp.update(|v| if v.contains(&id) { v.retain(|x| *x != id) } else { v.push(id) })>
                                        <span class="tr-name">{name}</span>
                                        <span class="tr-meta">{pos}" \u{2022} "{ovr}" \u{2022} "{sal}" \u{2022} val "{val}</span>
                                    </div>
                                }
                            }).collect_view()}
                            {move || picks(other.get()).into_iter().map(move |(id, label, val)| {
                                let picked = move || rk.get().contains(&id);
                                view! {
                                    <div class=move || if picked() { "trade-row pick picked" } else { "trade-row pick" }
                                        on:click=move |_| rk.update(|v| if v.contains(&id) { v.retain(|x| *x != id) } else { v.push(id) })>
                                        <span class="tr-name">"\u{1f4c4} "{label}</span>
                                        <span class="tr-meta">"pick \u{2022} val "{val}</span>
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
                                    {format!("Out ${:.1}M  \u{2194}  In ${:.1}M \u{2022} value {} \u{2194} {}", e.give_salary as f64 / 1000.0, e.get_salary as f64 / 1000.0, e.give_value.round() as i64, e.get_value.round() as i64)}
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
                    </div>
                </div>
            </Show>

            <Show when=move || !msg.get().is_empty()>
                <p class="offer-msg trade-status">{move || msg.get()}</p>
            </Show>
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
fn FinancesPanel() -> impl IntoView {
    let state = expect_context::<AppState>();
    let league = state.league;

    let fin = move || league.with(|l| l.user_team_id.and_then(|id| l.teams.iter().find(|t| t.id == id)).map(|t| t.finances.clone()));
    let proj = move || league.with(|l| l.user_team_id.map(|id| l.project_finances(id)));
    // Quiet update while dragging (live P&L, no serialization); persist on release.
    let commit = move |f: engine::Finances| state.update_league_quiet(move |l| {
        l.set_user_finances(f.ticket_price, f.concession_price, f.coaching, f.training, f.facilities, f.marketing);
    });
    let persist = move || state.persist();

    // Stadium.
    let can_up = move || league.with(|l| l.user_team_id.map(|id| l.can_upgrade_stadium(id)).unwrap_or(false));
    let up_cost = move || league.with(|l| l.user_team_id.map(|id| l.stadium_upgrade_cost(id)).unwrap_or(0));
    let upgrade = move |_| state.update_league(|l| { l.upgrade_stadium(); });

    // Merchandise: your players' jerseys, your team goods, league top sellers.
    let merch = move || league.with(|l| {
        let Some(uid) = l.user_team_id else { return (Vec::new(), 0u32, Vec::new()) };
        let mut mine: Vec<(u32, String, u32, u32)> = Vec::new();
        let mut jersey_total = 0u32;
        if let Some(team) = l.teams.iter().find(|t| t.id == uid) {
            for pid in &team.roster {
                let (u, r) = l.player_jersey_sales(*pid);
                if u == 0 { continue; }
                let name = l.players.iter().find(|p| p.id == *pid).map(|p| p.name.clone()).unwrap_or_default();
                jersey_total += r;
                mine.push((*pid, name, u, r));
            }
        }
        mine.sort_by(|a, b| b.2.cmp(&a.2));
        let team_goods = l.project_finances(uid).merch_rev.saturating_sub(jersey_total);
        let leaders: Vec<(String, String, u32, u32)> = l.league_jersey_leaders(10).into_iter().map(|(_, n, ab, u, r)| (n, ab, u, r)).collect();
        (mine, team_goods, leaders)
    });

    let m = |v: u32| format!("${:.1}M", v as f64 / 1000.0);
    let kfmt = |n: u32| if n >= 1000 { format!("{:.1}k", n as f64 / 1000.0) } else { n.to_string() };

    view! {
        <Show when=move || fin().is_some() fallback=|| view! { <div class="card"><p class="empty">"Pick a team first."</p></div> }>
            <div class="two-col">
                // ===== Profit & Loss =====
                <div class="card">
                    <h3 class="card-title">"Profit & Loss \u{2014} Projected"</h3>
                    {move || proj().map(|p| {
                        let att_pct = if p.capacity > 0 { p.attendance as f64 / p.capacity as f64 * 100.0 } else { 0.0 };
                        let fi = p.fan_interest * 100.0;
                        let over = p.expenses > p.budget;
                        view! {
                            <div class="fin-att">
                                <div class="fin-att-top">
                                    <span>"Fan Interest"</span><span class="dim">{format!("{:.0}%", fi)}</span>
                                </div>
                                <div class="fin-bar"><span class="fin-fill" style=format!("width:{}%", fi)></span></div>
                                <div class="fin-att-top" style="margin-top:.6rem">
                                    <span>{format!("Attendance {} / {}", p.attendance, p.capacity)}</span>
                                    <span class="dim">{format!("{:.0}% full", att_pct)}</span>
                                </div>
                                <div class="fin-bar"><span class="fin-fill" style=format!("width:{}%", att_pct)></span></div>
                                {p.unmet_demand.then(|| view! { <p class="fin-warn">"\u{26a0} Selling out and turning fans away \u{2014} expand the arena to capture the demand."</p> })}
                            </div>
                            <table class="tbl fin-pl">
                                <tbody>
                                    <tr class="row"><td class="left">"Gate receipts"</td><td>{m(p.ticket_rev)}</td></tr>
                                    <tr class="row"><td class="left">"Concessions"</td><td>{m(p.concession_rev)}</td></tr>
                                    <tr class="row"><td class="left">"Merchandise"</td><td>{m(p.merch_rev)}</td></tr>
                                    <tr class="row"><td class="left">"TV & sponsorship"</td><td>{m(p.tv_rev)}</td></tr>
                                    <tr class="row fin-sum rev"><td class="left">"Revenue"</td><td>{m(p.revenue)}</td></tr>
                                    <tr class="row"><td class="left">"Payroll"</td><td>{m(p.payroll)}</td></tr>
                                    <tr class="row"><td class="left">"Departments"</td><td>{m(p.budgets)}</td></tr>
                                    <tr class="row fin-sum exp"><td class="left">"Expenses"</td><td>{m(p.expenses)}</td></tr>
                                    <tr class="row"><td class="left">"Owner budget"</td><td>{m(p.budget)}</td></tr>
                                    <tr class="row fin-profit">
                                        <td class="left">"Projected profit"</td>
                                        <td class={if p.profit >= 0 { "pl-num good" } else { "pl-num bad" }}>
                                            {format!("{}${:.1}M", if p.profit < 0 { "-" } else { "" }, (p.profit.abs() as f64) / 1000.0)}
                                        </td>
                                    </tr>
                                </tbody>
                            </table>
                            {over.then(|| view! { <p class="fin-warn">"\u{26a0} Over the owner's budget \u{2014} trim payroll or department spending."</p> })}
                        }
                    })}
                </div>

                // ===== Controls =====
                <div class="card">
                    <h3 class="card-title">"Front Office"</h3>

                    <div class="fin-group-label">"Pricing"</div>
                    <div class="fin-ctl">
                        <label>"Ticket price"</label>
                        <input class="input fin-num" type="number" min="10" max="200" step="1"
                            prop:value=move || fin().map(|f| f.ticket_price).unwrap_or(0)
                            on:input=move |e| { if let Some(mut f) = fin() { f.ticket_price = event_target_value(&e).parse().unwrap_or(f.ticket_price); commit(f); } }
                            on:change=move |_| persist()/>
                        <span class="fin-unit">"$/seat"</span>
                    </div>
                    <div class="fin-ctl">
                        <label>"Concessions"</label>
                        <input class="input fin-num" type="number" min="5" max="60" step="1"
                            prop:value=move || fin().map(|f| f.concession_price).unwrap_or(0)
                            on:input=move |e| { if let Some(mut f) = fin() { f.concession_price = event_target_value(&e).parse().unwrap_or(f.concession_price); commit(f); } }
                            on:change=move |_| persist()/>
                        <span class="fin-unit">"$/fan"</span>
                    </div>

                    <div class="fin-group-label">"Department budgets"</div>
                    {fin_slider("Coaching", "faster player development", fin, commit, persist, Field::Coaching)}
                    {fin_slider("Training", "faster growth, slower decline", fin, commit, persist, Field::Training)}
                    {fin_slider("Facilities", "attendance + free-agent appeal", fin, commit, persist, Field::Facilities)}
                    {fin_slider("Marketing", "raises fan interest over time", fin, commit, persist, Field::Marketing)}

                    <div class="fin-group-label">"Stadium"</div>
                    {move || fin().map(|f| view! {
                        <div class="fin-stadium">
                            <div class="fin-stadium-info">{format!("{} seats \u{2022} {} years old", f.capacity, f.stadium_age)}</div>
                            <button class="btn" disabled=move || !can_up() on:click=upgrade>
                                {move || format!("Expand +3,000 ({})", m(up_cost()))}
                            </button>
                        </div>
                    })}
                    <p class="hint">{move || if can_up() { "The owner will fund an expansion.".to_string() } else { "The owner funds an expansion once you've earned it (high fan interest or sellouts).".to_string() }}</p>
                </div>
            </div>

            // ===== Merchandise =====
            <div class="card" style="margin-top:1.25rem">
                <h3 class="card-title">"Merchandise"</h3>
                <div class="two-col">
                    <div>
                        <div class="fin-group-label">"Your Jersey Sales"</div>
                        <table class="tbl">
                            <thead><tr><th class="left">"Player"</th><th>"Units"</th><th>"Revenue"</th></tr></thead>
                            <tbody>
                                {move || merch().0.into_iter().map(|(id, name, units, rev)| view! {
                                    <tr class="row">
                                        <td class="left"><crate::ui::PlayerLink id=id name=name/></td>
                                        <td>{kfmt(units)}</td>
                                        <td>{m(rev)}</td>
                                    </tr>
                                }).collect_view()}
                                <tr class="row fin-sum"><td class="left">"Team goods"</td><td></td><td>{move || m(merch().1)}</td></tr>
                            </tbody>
                        </table>
                    </div>
                    <div>
                        <div class="fin-group-label">"League Top Sellers"</div>
                        <table class="tbl">
                            <thead><tr><th>"#"</th><th class="left">"Player"</th><th>"Tm"</th><th>"Units"</th></tr></thead>
                            <tbody>
                                {move || merch().2.into_iter().enumerate().map(|(i, (name, ab, units, _r))| view! {
                                    <tr class="row">
                                        <td>{i + 1}</td><td class="left">{name}</td><td>{ab}</td><td>{kfmt(units)}</td>
                                    </tr>
                                }).collect_view()}
                            </tbody>
                        </table>
                    </div>
                </div>
            </div>
        </Show>
    }
}

#[derive(Clone, Copy)]
enum Field { Coaching, Training, Facilities, Marketing }

/// A budget slider in $M (stored as thousands). Higher = better effect, more cost.
fn fin_slider(
    label: &'static str,
    note: &'static str,
    fin: impl Fn() -> Option<engine::Finances> + Copy + Send + Sync + 'static,
    commit: impl Fn(engine::Finances) + Copy + Send + Sync + 'static,
    persist: impl Fn() + Copy + Send + Sync + 'static,
    field: Field,
) -> impl IntoView {
    let read = move || fin().map(|f| match field {
        Field::Coaching => f.coaching,
        Field::Training => f.training,
        Field::Facilities => f.facilities,
        Field::Marketing => f.marketing,
    }).unwrap_or(0);
    let write = move |v: u32| {
        if let Some(mut f) = fin() {
            match field {
                Field::Coaching => f.coaching = v,
                Field::Training => f.training = v,
                Field::Facilities => f.facilities = v,
                Field::Marketing => f.marketing = v,
            }
            commit(f);
        }
    };
    view! {
        <div class="fin-slider">
            <div class="fin-slider-top">
                <span class="fin-slider-label">{label}<span class="fin-slider-note">{note}</span></span>
                <span class="fin-slider-val">{move || format!("${:.1}M", read() as f64 / 1000.0)}</span>
            </div>
            <input class="fin-range" type="range" min="0" max="40" step="0.5"
                prop:value=move || read() as f64 / 1000.0
                on:input=move |e| write((event_target_value(&e).parse::<f64>().unwrap_or(0.0) * 1000.0) as u32)
                on:change=move |_| persist()/>
        </div>
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
                    (p.id, p.name.clone(), p.position.abbrev(), s.gp, s.mpg(), s.ppg(), s.rpg(), s.apg(), s.fg_pct(), s.tp_pct())
                })
                .collect();
            rows.sort_by(|a, b| b.5.partial_cmp(&a.5).unwrap());
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
                    (p.id, p.name.clone(), team, s.ppg())
                })
                .collect();
            rows.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap());
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
                            {move || team_rows().into_iter().map(|(id, name, pos, gp, mpg, ppg, rpg, apg, fg, tp)| view! {
                                <tr class="row">
                                    <td class="left"><crate::ui::PlayerLink id=id name=name/></td><td>{pos}</td><td>{gp}</td>
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
                            {move || leaders().into_iter().enumerate().map(|(i, (id, name, team, ppg))| view! {
                                <tr class="row">
                                    <td>{i + 1}</td>
                                    <td class="left"><crate::ui::PlayerLink id=id name=name/></td>
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
