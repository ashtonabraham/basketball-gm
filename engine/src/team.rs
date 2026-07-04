//! A team in the league.

use crate::player::Player;
use crate::types::{Color, Conference, PlayerId, TeamId};
use serde::{Deserialize, Serialize};

/// A team's front-office finances: what it charges fans and how it splits its
/// budget across departments. Prices are in whole dollars; department budgets
/// are in thousands of dollars per season (like salaries).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finances {
    pub ticket_price: u32,
    pub concession_price: u32,
    /// Speeds player development.
    pub coaching: u32,
    /// Speeds development and slows veteran decline.
    pub training: u32,
    /// Raises attendance and free-agent appeal.
    pub facilities: u32,
    /// Marketing spend: lifts fan interest over time.
    pub marketing: u32,
    /// 0..1 persistent fan interest, shown as a % bar. Grows with wins, playoff
    /// runs, star power, and marketing; drives attendance, merch, and TV money.
    pub fan_interest: f64,
    /// Stadium seats (upgradeable).
    pub capacity: u32,
    /// Seasons since the arena was built or last renovated.
    pub stadium_age: u32,
    /// How many expansions have been done (each one adds more seats + costs more).
    #[serde(default)]
    pub stadium_upgrades: u32,
    // Last completed season's booked figures (thousands), for the P&L.
    pub last_attendance: u32,
    pub last_revenue: u32,
    pub last_merch: u32,
    pub last_expenses: u32,
    pub last_profit: i64,
}

impl Default for Finances {
    fn default() -> Self {
        Finances {
            ticket_price: 60,
            concession_price: 25,
            coaching: 8_000,
            training: 8_000,
            facilities: 8_000,
            marketing: 6_000,
            fan_interest: 0.5,
            capacity: 18_000,
            stadium_age: 8,
            stadium_upgrades: 0,
            last_attendance: 0,
            last_revenue: 0,
            last_merch: 0,
            last_expenses: 0,
            last_profit: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Team {
    pub id: TeamId,
    /// Fixed in the team builder (one of the 32 preset locations).
    pub location: String,
    /// Editable nickname (defaults to the preset, e.g. "Celtics").
    pub name: String,
    pub abbrev: String,
    pub primary: Color,
    pub secondary: Color,
    pub conference: Conference,
    /// Ids of players on the roster.
    pub roster: Vec<PlayerId>,
    /// Regular-season record.
    pub wins: u32,
    pub losses: u32,
    /// Market size (~0.8 small .. ~1.3 big); drives arena size and TV money.
    #[serde(default = "default_market")]
    pub market: f64,
    /// Front-office finances.
    #[serde(default)]
    pub finances: Finances,
}

fn default_market() -> f64 {
    1.0
}

impl Team {
    /// Full display name, e.g. "Boston Celtics".
    pub fn full_name(&self) -> String {
        format!("{} {}", self.location, self.name)
    }

    pub fn games_played(&self) -> u32 {
        self.wins + self.losses
    }

    /// Arena capacity (stored; upgradeable via stadium expansion).
    pub fn capacity(&self) -> u32 {
        self.finances.capacity
    }

    pub fn win_pct(&self) -> f64 {
        let g = self.games_played();
        if g == 0 {
            0.0
        } else {
            self.wins as f64 / g as f64
        }
    }

    /// Team strength = average overall of the best 8 players (rotation depth).
    /// Used by the rating-based game simulator.
    pub fn strength(&self, players: &[Player]) -> f64 {
        let mut ovrs: Vec<u8> = self
            .roster
            .iter()
            .filter_map(|pid| players.iter().find(|p| p.id == *pid))
            .map(|p| p.overall())
            .collect();
        if ovrs.is_empty() {
            return 50.0;
        }
        ovrs.sort_unstable_by(|a, b| b.cmp(a));
        let n = ovrs.len().min(8);
        ovrs[..n].iter().map(|o| *o as f64).sum::<f64>() / n as f64
    }
}
