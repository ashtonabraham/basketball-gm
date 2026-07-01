//! Free-agency screen: the FA pool on the right, an offer panel for the
//! selected player, and round controls. Sign players by making the best offer.

use crate::state::{AppState, Tab};
use crate::ui::ThemeToggle;
use leptos::prelude::*;

#[component]
pub fn FreeAgencyScreen() -> impl IntoView {
    let state = expect_context::<AppState>();
    let league = state.league;

    let round = move || league.with(|l| l.free_agency.as_ref().map(|f| f.round).unwrap_or(1));
    let pool_empty = move || league.with(|l| l.free_agency.as_ref().map(|f| f.pool.is_empty()).unwrap_or(true));
    let cap_room = move || league.with(|l| {
        l.user_team_id.map(|id| l.team_cap_space(id) as f64 / 1000.0).unwrap_or(0.0)
    });
    let offers = move || league.with(|l| l.fa_offer_count());
    let log = move || league.with(|l| l.free_agency.as_ref().map(|f| f.log.clone()).unwrap_or_default());

    let sim_round = move |_| state.update_league(|l| l.fa_sim_round());
    let finish = move |_| {
        state.update_league(|l| l.fa_finish());
        state.tab.set(Tab::Standings);
    };

    view! {
        <div class="builder">
            <header class="builder-top">
                <div>
                    <h1 class="brand">"Free " <span class="brand-accent">"Agency"</span></h1>
                    <p class="subtitle">
                        {move || format!("Round {} \u{2022} Cap room: ${:.1}M \u{2022} Offers {}/{}", round(), cap_room(), offers(), engine::League::FA_MAX_OFFERS)}
                    </p>
                </div>
                <div class="draft-actions">
                    <button class="btn btn-primary" on:click=sim_round disabled=move || pool_empty()>"Sim Round"</button>
                    <button class="btn" on:click=finish>"Finish \u{2192} Start Season"</button>
                    <ThemeToggle/>
                </div>
            </header>

            <div class="draft-cols">
                <OfferPanel/>
                <FaPool/>
            </div>

            <Show when=move || !log().is_empty()>
                <div class="card fa-log">
                    <h3 class="card-title">{move || format!("Round {} signings", round().saturating_sub(1))}</h3>
                    <div class="fa-log-list">
                        {move || log().into_iter().map(|s| view! { <span class="fa-log-item">{s}</span> }).collect_view()}
                    </div>
                </div>
            </Show>

            <crate::ui::MyRosterCard/>
        </div>
    }
}

/// Local selection used by both panels.
#[derive(Clone, Copy)]
struct FaSelection(RwSignal<Option<u32>>);

#[component]
fn OfferPanel() -> impl IntoView {
    let state = expect_context::<AppState>();
    let league = state.league;
    let sel = FaSelection(RwSignal::new(None));
    provide_context(sel);

    let salary_m = RwSignal::new(0.0f64);
    let years = RwSignal::new(3u8);
    let msg = RwSignal::new(String::new());

    // When the selection changes, default the offer to the player's market value.
    Effect::new(move |_| {
        if let Some(pid) = sel.0.get() {
            let mkt = league.with(|l| l.players.iter().find(|p| p.id == pid).map(|p| engine::market_salary(p.overall())).unwrap_or(2000));
            salary_m.set((mkt as f64 / 1000.0 * 10.0).round() / 10.0);
            msg.set(String::new());
        }
    });

    let info = move || {
        sel.0.get().and_then(|pid| league.with(|l| {
            let p = l.players.iter().find(|p| p.id == pid)?;
            let user = l.user_team_id?;
            let existing = l.free_agency.as_ref().and_then(|f| f.user_offer(pid, user)).map(|o| (o.salary, o.years));
            Some((p.name.clone(), p.position.abbrev(), p.overall(), p.potential, existing))
        }))
    };

    let make_offer = move |_| {
        if let Some(pid) = sel.0.get() {
            let salary = (salary_m.get() * 1000.0).round() as u32;
            let ok = {
                let mut done = false;
                state.update_league(|l| done = l.fa_user_offer(pid, salary, years.get()));
                done
            };
            msg.set(if ok { "Offer submitted.".into() } else { "Not enough cap room or roster full.".into() });
        }
    };
    let withdraw = move |_| {
        if let Some(pid) = sel.0.get() {
            state.update_league(|l| l.fa_clear_user_offer(pid));
            msg.set("Offer withdrawn.".into());
        }
    };

    view! {
        <div class="card offer-panel">
            <h3 class="card-title">"Make an Offer"</h3>
            {move || match info() {
                None => view! { <p class="empty">"Select a free agent to make an offer."</p> }.into_any(),
                Some((name, pos, ovr, pot, existing)) => view! {
                    <div class="offer-player">
                        <div class="offer-name">{name}</div>
                        <div class="offer-meta">{pos}" \u{2022} "{ovr}" OVR \u{2022} "{pot}" POT"</div>
                    </div>
                    {existing.map(|(s, y)| view! {
                        <div class="offer-current">{format!("Your offer: ${:.1}M / {}yr", s as f64 / 1000.0, y)}</div>
                    })}
                    <div class="offer-form">
                        <label>"Salary ($M)"
                            <input class="input" type="number" step="0.5" min="1" max="48"
                                prop:value=move || salary_m.get()
                                on:input=move |e| salary_m.set(event_target_value(&e).parse().unwrap_or(1.0))/>
                        </label>
                        <label>"Years"
                            <input class="input" type="number" min="1" max="5"
                                prop:value=move || years.get()
                                on:input=move |e| years.set(event_target_value(&e).parse().unwrap_or(3))/>
                        </label>
                    </div>
                    <div class="offer-actions">
                        <button class="btn btn-primary" on:click=make_offer>"Make Offer"</button>
                        <button class="btn" on:click=withdraw>"Withdraw"</button>
                    </div>
                    <div class="offer-msg">{move || msg.get()}</div>
                }.into_any(),
            }}
        </div>
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Sort { Ovr, Pos, Age, Pot, Asking, Interest }

#[component]
fn FaPool() -> impl IntoView {
    let state = expect_context::<AppState>();
    let league = state.league;
    let sel = expect_context::<FaSelection>();
    // (column, ascending)
    let sort = RwSignal::new((Sort::Ovr, false));

    let pool = move || league.with(|l| {
        let Some(fa) = &l.free_agency else { return Vec::new() };
        let user = l.user_team_id;
        let mut v: Vec<_> = fa.pool.iter().filter_map(|pid| {
            let p = l.players.iter().find(|p| p.id == *pid)?;
            let offer = user.and_then(|u| fa.user_offer(*pid, u)).map(|o| o.salary);
            let interest = l.fa_interest(*pid);
            let irank = match interest {
                engine::Interest::Eager => 4, engine::Interest::Interested => 3,
                engine::Interest::Lukewarm => 2, engine::Interest::Unlikely => 1, engine::Interest::NoOffer => 0,
            };
            Some((p.id, p.name.clone(), p.position.abbrev(), p.age, p.overall(), p.potential,
                  engine::market_salary(p.overall()), offer, interest.label(), irank))
        }).collect();
        let (key, asc) = sort.get();
        v.sort_by(|a, b| {
            let o = match key {
                Sort::Ovr => a.4.cmp(&b.4),
                Sort::Pos => a.2.cmp(b.2),
                Sort::Age => a.3.cmp(&b.3),
                Sort::Pot => a.5.cmp(&b.5),
                Sort::Asking => a.6.cmp(&b.6),
                Sort::Interest => a.9.cmp(&b.9),
            };
            if asc { o } else { o.reverse() }
        });
        v
    });

    // Header that toggles sort on click.
    let th = move |label: &'static str, key: Sort, left: bool| {
        let cls = if left { "left sortable" } else { "sortable" };
        view! {
            <th class=cls on:click=move |_| sort.update(|(k, asc)| {
                if *k == key { *asc = !*asc; } else { *k = key; *asc = matches!(key, Sort::Pos | Sort::Age | Sort::Asking); }
            })>
                {label}{move || if sort.get().0 == key { if sort.get().1 { " \u{25b2}" } else { " \u{25bc}" } } else { "" }}
            </th>
        }
    };

    view! {
        <div class="card draft-prospects">
            <h3 class="card-title">"Free Agents"</h3>
            <table class="tbl">
                <thead><tr>
                    <th class="left">"Player"</th>
                    {th("Pos", Sort::Pos, false)}
                    {th("Age", Sort::Age, false)}
                    {th("OVR", Sort::Ovr, false)}
                    {th("POT", Sort::Pot, false)}
                    {th("Asking", Sort::Asking, false)}
                    <th>"Your Offer"</th>
                    {th("Interest", Sort::Interest, false)}
                </tr></thead>
                <tbody>
                    {move || pool().into_iter().map(move |(id, name, pos, age, ovr, pot, ask, offer, interest, irank)| {
                        let is_sel = move || sel.0.get() == Some(id);
                        let icls = match irank { 4 | 3 => "int good", 2 => "int mid", 1 => "int bad", _ => "int" };
                        view! {
                            <tr class=move || if is_sel() { "row pickable sel" } else { "row pickable" }
                                on:click=move |_| sel.0.set(Some(id))>
                                <td class="left"><crate::ui::PlayerLink id=id name=name/></td>
                                <td>{pos}</td>
                                <td>{age}</td>
                                <td><span class="ovr">{ovr}</span></td>
                                <td>{pot}</td>
                                <td>{format!("${:.1}M", ask as f64 / 1000.0)}</td>
                                <td>{offer.map(|s| format!("${:.1}M", s as f64 / 1000.0)).unwrap_or_else(|| "\u{2014}".into())}</td>
                                <td><span class=icls>{interest}</span></td>
                            </tr>
                        }
                    }).collect_view()}
                </tbody>
            </table>
        </div>
    }
}
