//! Draft screen: the lottery-seeded pick board on the left, the available
//! prospect board on the right. The user drafts on their pick; "Sim to my
//! pick" and "Sim entire draft" handle the rest.

use crate::state::AppState;
use crate::ui::ThemeToggle;
use leptos::prelude::*;

#[component]
pub fn DraftScreen() -> impl IntoView {
    let state = expect_context::<AppState>();
    let league = state.league;

    let season = move || league.with(|l| l.season);
    let complete = move || league.with(|l| l.draft_complete());
    let user_on = move || league.with(|l| l.is_user_on_clock());

    // Whose pick is it.
    let on_clock = move || {
        league.with(|l| {
            l.draft.as_ref().and_then(|d| d.current()).map(|p| {
                let team = l.teams.iter().find(|t| t.id == p.team_id).map(|t| t.full_name()).unwrap_or_default();
                (p.overall, team, Some(p.team_id) == l.user_team_id)
            })
        })
    };

    let status = move || match on_clock() {
        _ if complete() => "The draft is complete.".to_string(),
        Some((ov, _team, true)) => format!("You're on the clock — pick #{ov}"),
        Some((ov, team, false)) => format!("Pick #{ov}: {team} is on the clock"),
        None => String::new(),
    };

    // Actions.
    let sim_to_me = move |_| state.update_league(|l| l.draft_sim_to_user());
    let sim_all = move |_| state.update_league(|l| l.draft_sim_all());
    let to_fa = move |_| state.update_league(|l| l.enter_free_agency());

    view! {
        <div class="builder">
            <header class="builder-top">
                <div>
                    <h1 class="brand">"Rookie " <span class="brand-accent">"Draft"</span></h1>
                    <p class="subtitle">{move || format!("Following the {} season", season())}" \u{2022} "{status}</p>
                </div>
                <div class="draft-actions">
                    <Show
                        when=complete
                        fallback=move || view! {
                            <Show when=move || !user_on()>
                                <button class="btn" on:click=sim_to_me>"Sim to My Pick"</button>
                            </Show>
                            <button class="btn" on:click=sim_all>"Sim Entire Draft"</button>
                        }
                    >
                        <button class="btn btn-primary" on:click=to_fa>
                            "Continue to Free Agency \u{2192}"
                        </button>
                    </Show>
                    <ThemeToggle/>
                </div>
            </header>

            <div class="draft-cols">
                <PickBoard/>
                <ProspectBoard/>
            </div>

            <crate::ui::MyRosterCard/>
        </div>
    }
}

#[component]
fn PickBoard() -> impl IntoView {
    let state = expect_context::<AppState>();
    let league = state.league;
    let picks = move || {
        league.with(|l| {
            let Some(d) = &l.draft else { return Vec::new() };
            d.picks
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let team = l.teams.iter().find(|t| t.id == p.team_id).map(|t| t.abbrev.clone()).unwrap_or_default();
                    let is_user = Some(p.team_id) == l.user_team_id;
                    let name = p.player_id.and_then(|pid| l.players.iter().find(|pl| pl.id == pid)).map(|pl| format!("{} ({})", pl.name, pl.position.abbrev()));
                    (p.overall, p.round, team, name, i == d.on_clock, is_user)
                })
                .collect::<Vec<_>>()
        })
    };

    view! {
        <div class="card draft-board">
            <h3 class="card-title">"Draft Order"</h3>
            <div class="pick-list">
                {move || picks().into_iter().map(|(ov, round, team, name, current, is_user)| {
                    let cls = if current { "pick-row current" } else if is_user { "pick-row user" } else { "pick-row" };
                    let label = name.unwrap_or_else(|| if current { "On the clock".into() } else { "\u{2014}".into() });
                    view! {
                        <div class=cls>
                            <span class="pick-no">{ov}</span>
                            <span class="pick-team">{team}</span>
                            <span class="pick-name">{label}</span>
                            <span class="pick-round">{format!("R{round}")}</span>
                        </div>
                    }
                }).collect_view()}
            </div>
        </div>
    }
}

#[derive(Clone, Copy, PartialEq)]
enum DSort { Ovr, Pos, Age, Pot }

#[component]
fn ProspectBoard() -> impl IntoView {
    let state = expect_context::<AppState>();
    let league = state.league;
    let user_on = move || league.with(|l| l.is_user_on_clock());
    let scout_left = move || league.with(|l| l.draft.as_ref().map(|d| d.scout_points).unwrap_or(0));
    let sort = RwSignal::new((DSort::Ovr, false));

    let prospects = move || {
        league.with(|l| {
            let Some(d) = &l.draft else { return Vec::new() };
            let mut v: Vec<_> = d
                .prospects
                .iter()
                .filter_map(|id| l.players.iter().find(|p| p.id == *id))
                .map(|p| {
                    let r = &p.ratings;
                    // Scouted potential = a fuzzy letter grade until drafted.
                    let (grade, conf, est) = d
                        .scouting
                        .get(&p.id)
                        .map(|s| (s.grade().to_string(), s.confidence(), s.estimate))
                        .unwrap_or_else(|| ("?".into(), 0, 0.0));
                    (p.id, p.name.clone(), p.position.abbrev(), p.age, p.overall(), grade, conf, est,
                     r.three, r.layup, r.dunk, r.passing, r.ball_handling, r.rebounding, r.defense, r.athleticism)
                })
                .collect();
            let (key, asc) = sort.get();
            v.sort_by(|a, b| {
                let o = match key {
                    DSort::Ovr => a.4.cmp(&b.4),
                    DSort::Pos => a.2.cmp(b.2),
                    DSort::Age => a.3.cmp(&b.3),
                    DSort::Pot => a.7.partial_cmp(&b.7).unwrap(),
                };
                if asc { o } else { o.reverse() }
            });
            v
        })
    };

    let pick = move |pid: u32| state.update_league(move |l| l.draft_user_pick(pid));
    let scout = move |pid: u32| state.update_league(move |l| l.scout_prospect(pid));

    let th = move |label: &'static str, key: DSort| view! {
        <th class="sortable" on:click=move |_| sort.update(|(k, asc)| {
            if *k == key { *asc = !*asc; } else { *k = key; *asc = matches!(key, DSort::Pos | DSort::Age); }
        })>
            {label}{move || if sort.get().0 == key { if sort.get().1 { " \u{25b2}" } else { " \u{25bc}" } } else { "" }}
        </th>
    };

    view! {
        <div class="card draft-prospects">
            <h3 class="card-title">
                "Available Prospects"
                <span class="pick-hint">{move || format!(" — Scouting: {} left", scout_left())}</span>
            </h3>
            <table class="tbl">
                <thead><tr>
                    <th class="left">"Prospect"</th>
                    {th("Pos", DSort::Pos)}{th("Age", DSort::Age)}{th("OVR", DSort::Ovr)}{th("POT", DSort::Pot)}
                    <th>"3pt"</th><th>"Lay"</th><th>"Dnk"</th><th>"Pas"</th><th>"Hdl"</th>
                    <th>"Reb"</th><th>"Def"</th><th>"Ath"</th><th></th>
                </tr></thead>
                <tbody>
                    {move || {
                        let can_pick = user_on();
                        let can_scout = scout_left() > 0;
                        prospects().into_iter().map(move |(id, name, pos, age, ovr, grade, conf, _est, three, lay, dnk, pas, hdl, reb, def, ath)| {
                            let dots = "\u{25cf}".repeat(conf as usize) + &"\u{25cb}".repeat(3 - conf as usize);
                            view! {
                                <tr class="row">
                                    <td class="left">{name}</td>
                                    <td>{pos}</td>
                                    <td>{age}</td>
                                    <td><span class="ovr">{ovr}</span></td>
                                    <td>
                                        <span class="grade">{grade}</span>
                                        <span class="conf" title="Scouting confidence">{dots}</span>
                                    </td>
                                    <td>{three}</td><td>{lay}</td><td>{dnk}</td>
                                    <td>{pas}</td><td>{hdl}</td>
                                    <td>{reb}</td><td>{def}</td><td>{ath}</td>
                                    <td class="prospect-actions">
                                        <Show when=move || can_scout && conf < 3>
                                            <button class="mini-btn" title="Spend a scouting point" on:click=move |_| scout(id)>"Scout"</button>
                                        </Show>
                                        <Show when=move || can_pick>
                                            <button class="mini-btn draft" on:click=move |_| pick(id)>"Draft"</button>
                                        </Show>
                                    </td>
                                </tr>
                            }
                        }).collect_view()
                    }}
                </tbody>
            </table>
        </div>
    }
}
