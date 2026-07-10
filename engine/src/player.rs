//! Players, their ratings, and accumulated season statistics.
//!
//! Ratings are individual 0–100 attributes. They are intentionally granular so
//! the possession simulator can let each one genuinely affect outcomes (e.g.
//! `ball_handling` helps a player beat his defender, producing more makes and
//! fewer turnovers). When adding new game systems, prefer wiring them to these
//! attributes so everything carries real weight.

use crate::types::{PlayerId, Position, TeamId};
use serde::{Deserialize, Serialize};

/// A player's skill attributes, each 0–100. Granular on purpose: the possession
/// simulator reads these individually so every one carries real weight. Grouped
/// (in display and in the composite helpers) into inside scoring, outside
/// shooting, playmaking, defense, and physicals.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ratings {
    // --- Inside scoring ---
    /// Finishing at the rim off drives / floaters.
    pub layup: u8,
    /// Finishing above the rim; rewards athleticism on drives.
    pub dunk: u8,
    /// Back-to-the-basket post scoring (bigs).
    pub post: u8,
    // --- Outside shooting ---
    /// Mid-range jumper.
    pub mid_range: u8,
    /// Three-point shooting.
    pub three: u8,
    /// Free-throw accuracy.
    pub free_throw: u8,
    // --- Playmaking ---
    /// Passing — drives assist creation and assist accuracy.
    pub passing: u8,
    /// Ball handling — beating defenders off the dribble; reduces turnovers.
    pub ball_handling: u8,
    /// Decision-making; reduces turnovers and sharpens shot selection/assists.
    pub basketball_iq: u8,
    // --- Defense ---
    /// Rim protection and defending inside shots.
    pub interior_defense: u8,
    /// On-ball perimeter defense; contests jumpers and drives.
    pub perimeter_defense: u8,
    /// Creating steals.
    pub steal: u8,
    /// Blocking shots.
    pub block: u8,
    /// Grabbing rebounds on both glasses.
    pub rebounding: u8,
    // --- Physicals ---
    /// Speed / strength / leaping; aids drives, finishing, steals, blocks.
    pub athleticism: u8,
    /// Endurance; carries a bigger minutes load without fading.
    pub stamina: u8,
}

impl Ratings {
    /// A single 0–100 summary used for sorting and roster strength. Weighted
    /// toward the attributes that move the needle most in the sim.
    pub fn overall(&self) -> u8 {
        let s = self.layup as f64 * 0.9
            + self.dunk as f64 * 0.6
            + self.post as f64 * 0.5
            + self.mid_range as f64 * 0.8
            + self.three as f64 * 1.1
            + self.free_throw as f64 * 0.3
            + self.passing as f64 * 0.85
            + self.ball_handling as f64 * 0.85
            + self.basketball_iq as f64 * 0.7
            + self.interior_defense as f64 * 0.8
            + self.perimeter_defense as f64 * 0.9
            + self.steal as f64 * 0.5
            + self.block as f64 * 0.5
            + self.rebounding as f64 * 0.7
            + self.athleticism as f64 * 0.7
            + self.stamina as f64 * 0.3;
        // Sum of the weights above.
        const W: f64 = 11.0;
        (s / W).round() as u8
    }

    /// Composite scoring punch — used to weight how often a player is used on
    /// offense (his "usage").
    pub fn scoring(&self) -> f64 {
        (self.layup as f64 + self.dunk as f64 + self.post as f64 + self.mid_range as f64 + self.three as f64) / 5.0
            + self.ball_handling as f64 * 0.18
    }

    fn avg(vals: &[u8]) -> u8 {
        (vals.iter().map(|v| *v as u32).sum::<u32>() as f64 / vals.len() as f64).round() as u8
    }

    /// Inside-scoring composite (layup / dunk / post).
    pub fn inside(&self) -> u8 {
        Self::avg(&[self.layup, self.dunk, self.post])
    }
    /// Outside-shooting composite (mid-range / three / free throw).
    pub fn outside(&self) -> u8 {
        Self::avg(&[self.mid_range, self.three, self.free_throw])
    }
    /// Playmaking composite (passing / handle / IQ).
    pub fn playmaking(&self) -> u8 {
        Self::avg(&[self.passing, self.ball_handling, self.basketball_iq])
    }
    /// Defense composite (interior / perimeter / steal / block / rebounding).
    pub fn defending(&self) -> u8 {
        Self::avg(&[self.interior_defense, self.perimeter_defense, self.steal, self.block, self.rebounding])
    }
    /// Physical composite (athleticism / stamina).
    pub fn athletic(&self) -> u8 {
        Self::avg(&[self.athleticism, self.stamina])
    }
}

/// A player's contract. Salary is in thousands of dollars per year (so 28000
/// means $28.0M). `years` counts seasons remaining, including the upcoming one;
/// 0 means the player is a free agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    pub salary: u32,
    pub years: u8,
}

impl Contract {
    pub fn free_agent() -> Self {
        Contract { salary: 0, years: 0 }
    }
    pub fn is_expired(&self) -> bool {
        self.years == 0
    }
    /// Salary formatted like "$28.0M".
    pub fn salary_str(&self) -> String {
        format!("${:.1}M", self.salary as f64 / 1000.0)
    }
}

/// A player's personality — a single defining trait that gives the roster human
/// texture and real consequences (development, locker-room chemistry, morale,
/// merch, and trade demands).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlayerTrait {
    /// Lifts team chemistry and teammates; morale rides on winning.
    Leader,
    /// Steady, content, sticks around; morale barely wavers.
    Loyal,
    /// Chases money and winning; sours fast when losing or underpaid.
    Mercenary,
    /// Volatile; a losing locker room drags him (and the team) down.
    Hothead,
    /// Relentless worker — develops noticeably faster.
    GymRat,
    /// Makes the young players around him better.
    Mentor,
    /// A magnet for the fanbase — sells jerseys, lifts fan interest.
    FanFavorite,
    /// Even-keeled pro; coachable, no drama.
    Professional,
}

impl PlayerTrait {
    pub const ALL: [PlayerTrait; 8] = [
        PlayerTrait::Leader, PlayerTrait::Loyal, PlayerTrait::Mercenary, PlayerTrait::Hothead,
        PlayerTrait::GymRat, PlayerTrait::Mentor, PlayerTrait::FanFavorite, PlayerTrait::Professional,
    ];
    pub fn label(&self) -> &'static str {
        match self {
            PlayerTrait::Leader => "Leader",
            PlayerTrait::Loyal => "Loyal",
            PlayerTrait::Mercenary => "Mercenary",
            PlayerTrait::Hothead => "Hothead",
            PlayerTrait::GymRat => "Gym Rat",
            PlayerTrait::Mentor => "Mentor",
            PlayerTrait::FanFavorite => "Fan Favorite",
            PlayerTrait::Professional => "Professional",
        }
    }
    pub fn blurb(&self) -> &'static str {
        match self {
            PlayerTrait::Leader => "Lifts the locker room; his mood follows winning.",
            PlayerTrait::Loyal => "Content and steady — rarely rocks the boat.",
            PlayerTrait::Mercenary => "In it for money and rings; sours quickly otherwise.",
            PlayerTrait::Hothead => "Volatile — a losing culture drags him and the team down.",
            PlayerTrait::GymRat => "Outworks everyone; develops faster than his peers.",
            PlayerTrait::Mentor => "Makes the young players around him better.",
            PlayerTrait::FanFavorite => "The city loves him — sells jerseys and fills seats.",
            PlayerTrait::Professional => "Even-keeled and coachable; no drama.",
        }
    }
}

fn default_trait() -> PlayerTrait {
    PlayerTrait::Professional
}
fn default_morale() -> f64 {
    0.55
}

/// A player.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Player {
    pub id: PlayerId,
    pub name: String,
    pub age: u8,
    pub position: Position,
    pub ratings: Ratings,
    /// Defining personality trait.
    #[serde(default = "default_trait")]
    pub personality: PlayerTrait,
    /// 0..1 morale (happiness). Low morale hurts play and can trigger a trade request.
    #[serde(default = "default_morale")]
    pub morale: f64,
    /// Games remaining on a suspension (the rare arrest); 0 = available.
    #[serde(default)]
    pub suspended: u32,
    /// True peak overall this player can reach (the development ceiling). This
    /// is the exact value shown on rosters; for undrafted prospects it is
    /// hidden behind a scouted letter grade.
    pub potential: u8,
    /// `None` for free agents / undrafted prospects.
    pub team: Option<TeamId>,
    /// The season in which this player was drafted (used to identify rookies).
    /// `None` for the initial league population.
    pub draft_season: Option<u32>,
    /// Current contract (salary + years remaining).
    pub contract: Contract,
}

impl Player {
    pub fn overall(&self) -> u8 {
        self.ratings.overall()
    }

    /// Whether the player still has untapped upside.
    pub fn has_upside(&self) -> bool {
        self.potential > self.overall()
    }
}

/// Accumulated regular-season totals for one player. Stored separately from
/// `Player` (indexed by player id) so the simulator can read rosters
/// immutably while writing stats.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SeasonStats {
    pub gp: u32,
    pub min: u32,
    pub pts: u32,
    pub fgm: u32,
    pub fga: u32,
    pub tpm: u32,
    pub tpa: u32,
    pub ftm: u32,
    pub fta: u32,
    pub oreb: u32,
    pub dreb: u32,
    pub ast: u32,
    pub stl: u32,
    pub blk: u32,
    pub tov: u32,
}

impl SeasonStats {
    pub fn reb(&self) -> u32 {
        self.oreb + self.dreb
    }

    fn per_game(&self, total: u32) -> f64 {
        if self.gp == 0 {
            0.0
        } else {
            total as f64 / self.gp as f64
        }
    }

    pub fn ppg(&self) -> f64 {
        self.per_game(self.pts)
    }
    pub fn rpg(&self) -> f64 {
        self.per_game(self.reb())
    }
    pub fn apg(&self) -> f64 {
        self.per_game(self.ast)
    }
    pub fn mpg(&self) -> f64 {
        self.per_game(self.min)
    }

    pub fn spg(&self) -> f64 {
        self.per_game(self.stl)
    }
    pub fn bpg(&self) -> f64 {
        self.per_game(self.blk)
    }
    pub fn tovpg(&self) -> f64 {
        self.per_game(self.tov)
    }
    pub fn orpg(&self) -> f64 {
        self.per_game(self.oreb)
    }
    pub fn drpg(&self) -> f64 {
        self.per_game(self.dreb)
    }

    pub fn fg_pct(&self) -> f64 {
        if self.fga == 0 { 0.0 } else { self.fgm as f64 / self.fga as f64 }
    }
    pub fn tp_pct(&self) -> f64 {
        if self.tpa == 0 { 0.0 } else { self.tpm as f64 / self.tpa as f64 }
    }
    pub fn ft_pct(&self) -> f64 {
        if self.fta == 0 { 0.0 } else { self.ftm as f64 / self.fta as f64 }
    }

    /// Add another season's totals into these (for career totals).
    pub fn add(&mut self, o: &SeasonStats) {
        self.gp += o.gp;
        self.min += o.min;
        self.pts += o.pts;
        self.fgm += o.fgm;
        self.fga += o.fga;
        self.tpm += o.tpm;
        self.tpa += o.tpa;
        self.ftm += o.ftm;
        self.fta += o.fta;
        self.oreb += o.oreb;
        self.dreb += o.dreb;
        self.ast += o.ast;
        self.stl += o.stl;
        self.blk += o.blk;
        self.tov += o.tov;
    }
}

/// An honor a player earned in a given season, for the career/accomplishments
/// view. Accumulates over a career (e.g. "3× Champion").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Honor {
    Mvp,
    Dpoy,
    Roy,
    FinalsMvp,
    Champion,
}

impl Honor {
    /// Full name, e.g. "Most Valuable Player".
    pub fn label(&self) -> &'static str {
        match self {
            Honor::Mvp => "Most Valuable Player",
            Honor::Dpoy => "Defensive Player of the Year",
            Honor::Roy => "Rookie of the Year",
            Honor::FinalsMvp => "Finals MVP",
            Honor::Champion => "Champion",
        }
    }
    /// Short tag, e.g. "MVP".
    pub fn short(&self) -> &'static str {
        match self {
            Honor::Mvp => "MVP",
            Honor::Dpoy => "DPOY",
            Honor::Roy => "ROY",
            Honor::FinalsMvp => "Finals MVP",
            Honor::Champion => "Champion",
        }
    }
}

/// An honor tagged with the season it was won.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct HonorEntry {
    pub season: u32,
    pub honor: Honor,
}

/// One completed season in a player's career log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CareerSeason {
    pub season: u32,
    pub team_abbrev: String,
    pub age: u8,
    /// The player's overall at the end of that season.
    pub overall: u8,
    pub stats: SeasonStats,
}

/// A player's accumulated career: season-by-season log plus honors won.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Career {
    pub seasons: Vec<CareerSeason>,
    pub honors: Vec<HonorEntry>,
}

impl Career {
    /// Summed totals across every recorded season.
    pub fn totals(&self) -> SeasonStats {
        let mut t = SeasonStats::default();
        for cs in &self.seasons {
            t.add(&cs.stats);
        }
        t
    }

    /// How many times a given honor was won.
    pub fn count(&self, honor: Honor) -> usize {
        self.honors.iter().filter(|h| h.honor == honor).count()
    }
}
