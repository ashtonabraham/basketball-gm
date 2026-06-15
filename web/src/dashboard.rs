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
            let mut v: Vec<_> = l.schedule.iter().filter(|g| g.home == id || g.away == id).collect();
            v.sort_by_key(|g| g.day);
            v.into_iter().map(|g| {
                let home = g.home == id;
                let opp_id = if home { g.away } else { g.home };
                let opp = l.teams.iter().find(|t| t.id == opp_id).map(|t| t.abbrev.clone()).unwrap_or_default();
                let result = g.result.map(|r| {
                    let (us, them) = if home { (r.home_score, r.away_score) } else { (r.away_score, r.home_score) };
                    (us > them, us, them)
                });
                (g.day + 1, home, opp, result)
            }).collect::<Vec<_>>()
        })
    };

    view! {
        <div class="card">
            <h3 class="card-title">"Schedule"</h3>
            <table class="tbl">
                <thead><tr><th>"Day"</th><th class="left">"Opponent"</th><th class="left">"Result"</th></tr></thead>
                <tbody>
                    {move || games().into_iter().map(|(day, home, opp, result)| {
                        let loc = if home { "vs" } else { "@" };
                        let (cls, txt) = match result {
                            Some((win, us, them)) => (
                                if win { "row win" } else { "row loss" },
                                format!("{} {}\u{2013}{}", if win { "W" } else { "L" }, us, them),
                            ),
                            None => ("row", "\u{2014}".to_string()),
                        };
                        view! {
                            <tr class=cls>
                                <td>{day}</td>
                                <td class="left">{loc}" "{opp}</td>
                                <td class="left">{txt}</td>
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
        </div>
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
                     r.layup, r.dunk, r.three, r.passing, r.ball_handling,
                     r.rebounding, r.defense, r.athleticism)
                })
                .collect();
            ps.sort_by(|a, b| b.3.cmp(&a.3));
            ps
        })
    };

    view! {
        <div class="card">
            <h3 class="card-title">"Roster"</h3>
            <table class="tbl">
                <thead><tr>
                    <th class="left">"Player"</th><th>"Pos"</th><th>"Age"</th><th>"OVR"</th>
                    <th title="Potential (peak overall)">"POT"</th>
                    <th title="Layup">"Lay"</th><th title="Dunk">"Dnk"</th><th title="Three-point">"3pt"</th>
                    <th title="Passing">"Pas"</th><th title="Ball handling">"Hdl"</th>
                    <th title="Rebounding">"Reb"</th><th title="Defense">"Def"</th><th title="Athleticism">"Ath"</th>
                </tr></thead>
                <tbody>
                    {move || players().into_iter().map(|(name, pos, age, ovr, pot, lay, dnk, three, pas, hdl, reb, def, ath)| {
                        let upside = pot > ovr;
                        view! {
                            <tr class="row">
                                <td class="left">{name}</td>
                                <td>{pos}</td>
                                <td>{age}</td>
                                <td><span class="ovr">{ovr}</span></td>
                                <td><span class=if upside { "pot up" } else { "pot" }>{pot}</span></td>
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
fn PlayoffsPanel() -> impl IntoView {
    let state = expect_context::<AppState>();
    let has_po = move || state.league.with(|l| l.playoffs.is_some());

    // Each series carries both teams' badge (abbrev + colors) and series score.
    let rounds = move || {
        state.league.with(|l| {
            let badge = |id: u32| {
                l.teams.iter().find(|t| t.id == id).map(|t| {
                    (t.abbrev.clone(), t.primary.hex().to_string(), t.secondary.hex().to_string())
                }).unwrap_or_default()
            };
            let Some(po) = &l.playoffs else { return Vec::new() };
            po.rounds.iter().enumerate().map(|(ri, round)| {
                let series = round.iter().map(|s| {
                    let winner = s.winner();
                    (badge(s.high), s.high_wins, badge(s.low), s.low_wins,
                     winner == Some(s.high), winner == Some(s.low))
                }).collect::<Vec<_>>();
                (ROUND_NAMES.get(ri).copied().unwrap_or(""), series)
            }).collect::<Vec<_>>()
        })
    };
    let champ = move || state.league.with(|l| {
        l.playoffs.as_ref().and_then(|p| p.champion)
            .and_then(|id| l.teams.iter().find(|t| t.id == id))
            .map(|t| t.full_name())
    });

    view! {
        <Show
            when=has_po
            fallback=|| view! { <div class="card"><p class="empty">"The playoffs haven\u{2019}t started yet. Finish the regular season first."</p></div> }
        >
            <div class="card">
                <h3 class="card-title">"Playoff Bracket"</h3>
                {move || champ().map(|c| view! { <div class="champ-banner">"\u{1f3c6} "{c}" \u{2014} Champions"</div> })}
                <div class="bracket">
                    {move || rounds().into_iter().map(|(rname, series)| {
                        view! {
                            <div class="bracket-round">
                                <h4 class="round-name">{rname}</h4>
                                {series.into_iter().map(|((ha, hc1, hc2), hw, (la, lc1, lc2), lw, hi_won, lo_won)| {
                                    view! {
                                        <div class="series">
                                            <div class=if hi_won { "seed-line won" } else { "seed-line" }>
                                                <span class="mini-logo" style=format!("--c1:{};--c2:{}", hc1, hc2)>{ha.clone()}</span>
                                                <span class="seed-name">{ha}</span>
                                                <span class="wins">{hw}</span>
                                            </div>
                                            <div class=if lo_won { "seed-line won" } else { "seed-line" }>
                                                <span class="mini-logo" style=format!("--c1:{};--c2:{}", lc1, lc2)>{la.clone()}</span>
                                                <span class="seed-name">{la}</span>
                                                <span class="wins">{lw}</span>
                                            </div>
                                        </div>
                                    }
                                }).collect_view()}
                            </div>
                        }
                    }).collect_view()}
                </div>
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
