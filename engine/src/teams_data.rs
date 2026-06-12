//! Preset team data: 30 real NBA locations + Cincinnati + Seattle = 32 teams.
//!
//! The user picks a location in the team builder. The location is fixed, but
//! the name and colors come pre-filled and can be edited.

use crate::types::{Color, Conference};

/// An editable starting point for a team. Location is fixed; name/colors are
/// defaults the user may change.
#[derive(Debug, Clone)]
pub struct TeamPreset {
    pub location: &'static str,
    pub name: &'static str,
    pub abbrev: &'static str,
    pub primary: &'static str,
    pub secondary: &'static str,
    pub conference: Conference,
}

impl TeamPreset {
    pub fn primary_color(&self) -> Color {
        Color::new(self.primary)
    }
    pub fn secondary_color(&self) -> Color {
        Color::new(self.secondary)
    }
}

use Conference::{East, West};

/// All 32 presets. Cincinnati (East) and Seattle (West) round out the league
/// to 16 teams per conference.
pub const PRESETS: &[TeamPreset] = &[
    // ----- Eastern Conference (16) -----
    TeamPreset { location: "Atlanta",      name: "Hawks",        abbrev: "ATL", primary: "#E03A3E", secondary: "#26282A", conference: East },
    TeamPreset { location: "Boston",       name: "Celtics",      abbrev: "BOS", primary: "#007A33", secondary: "#BA9653", conference: East },
    TeamPreset { location: "Brooklyn",     name: "Nets",         abbrev: "BKN", primary: "#000000", secondary: "#FFFFFF", conference: East },
    TeamPreset { location: "Charlotte",    name: "Hornets",      abbrev: "CHA", primary: "#1D1160", secondary: "#00788C", conference: East },
    TeamPreset { location: "Chicago",      name: "Bulls",        abbrev: "CHI", primary: "#CE1141", secondary: "#000000", conference: East },
    TeamPreset { location: "Cincinnati",   name: "Royals",       abbrev: "CIN", primary: "#5B2B82", secondary: "#F2C75C", conference: East },
    TeamPreset { location: "Cleveland",    name: "Cavaliers",    abbrev: "CLE", primary: "#860038", secondary: "#FDBB30", conference: East },
    TeamPreset { location: "Detroit",      name: "Pistons",      abbrev: "DET", primary: "#C8102E", secondary: "#1D42BA", conference: East },
    TeamPreset { location: "Indiana",      name: "Pacers",       abbrev: "IND", primary: "#002D62", secondary: "#FDBB30", conference: East },
    TeamPreset { location: "Miami",        name: "Heat",         abbrev: "MIA", primary: "#98002E", secondary: "#F9A01B", conference: East },
    TeamPreset { location: "Milwaukee",    name: "Bucks",        abbrev: "MIL", primary: "#00471B", secondary: "#EEE1C6", conference: East },
    TeamPreset { location: "New York",     name: "Knicks",       abbrev: "NYK", primary: "#006BB6", secondary: "#F58426", conference: East },
    TeamPreset { location: "Orlando",      name: "Magic",        abbrev: "ORL", primary: "#0077C0", secondary: "#C4CED4", conference: East },
    TeamPreset { location: "Philadelphia", name: "76ers",        abbrev: "PHI", primary: "#006BB6", secondary: "#ED174C", conference: East },
    TeamPreset { location: "Toronto",      name: "Raptors",      abbrev: "TOR", primary: "#CE1141", secondary: "#000000", conference: East },
    TeamPreset { location: "Washington",   name: "Wizards",      abbrev: "WAS", primary: "#002B5C", secondary: "#E31837", conference: East },
    // ----- Western Conference (16) -----
    TeamPreset { location: "Dallas",       name: "Mavericks",    abbrev: "DAL", primary: "#00538C", secondary: "#002B5E", conference: West },
    TeamPreset { location: "Denver",       name: "Nuggets",      abbrev: "DEN", primary: "#0E2240", secondary: "#FEC524", conference: West },
    TeamPreset { location: "Golden State", name: "Warriors",     abbrev: "GSW", primary: "#1D428A", secondary: "#FFC72C", conference: West },
    TeamPreset { location: "Houston",      name: "Rockets",      abbrev: "HOU", primary: "#CE1141", secondary: "#000000", conference: West },
    TeamPreset { location: "Los Angeles",  name: "Clippers",     abbrev: "LAC", primary: "#C8102E", secondary: "#1D428A", conference: West },
    TeamPreset { location: "L.A.",         name: "Lakers",       abbrev: "LAL", primary: "#552583", secondary: "#FDB927", conference: West },
    TeamPreset { location: "Memphis",      name: "Grizzlies",    abbrev: "MEM", primary: "#5D76A9", secondary: "#12173F", conference: West },
    TeamPreset { location: "Minnesota",    name: "Timberwolves", abbrev: "MIN", primary: "#0C2340", secondary: "#236192", conference: West },
    TeamPreset { location: "New Orleans",  name: "Pelicans",     abbrev: "NOP", primary: "#0C2340", secondary: "#C8102E", conference: West },
    TeamPreset { location: "Oklahoma City",name: "Thunder",      abbrev: "OKC", primary: "#007AC1", secondary: "#EF3B24", conference: West },
    TeamPreset { location: "Phoenix",      name: "Suns",         abbrev: "PHX", primary: "#1D1160", secondary: "#E56020", conference: West },
    TeamPreset { location: "Portland",     name: "Trail Blazers",abbrev: "POR", primary: "#E03A3E", secondary: "#000000", conference: West },
    TeamPreset { location: "Sacramento",   name: "Kings",        abbrev: "SAC", primary: "#5A2D81", secondary: "#63727A", conference: West },
    TeamPreset { location: "San Antonio",  name: "Spurs",        abbrev: "SAS", primary: "#C4CED4", secondary: "#000000", conference: West },
    TeamPreset { location: "Seattle",      name: "SuperSonics",  abbrev: "SEA", primary: "#00653A", secondary: "#FFC200", conference: West },
    TeamPreset { location: "Utah",         name: "Jazz",         abbrev: "UTA", primary: "#002B5C", secondary: "#F9A01B", conference: West },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_32_teams_balanced_16_16() {
        assert_eq!(PRESETS.len(), 32);
        let east = PRESETS.iter().filter(|p| p.conference == East).count();
        let west = PRESETS.iter().filter(|p| p.conference == West).count();
        assert_eq!(east, 16);
        assert_eq!(west, 16);
    }

    #[test]
    fn includes_cincinnati_and_seattle() {
        assert!(PRESETS.iter().any(|p| p.location == "Cincinnati"));
        assert!(PRESETS.iter().any(|p| p.location == "Seattle"));
    }

    #[test]
    fn abbrevs_unique() {
        let mut seen = std::collections::HashSet::new();
        for p in PRESETS {
            assert!(seen.insert(p.abbrev), "duplicate abbrev {}", p.abbrev);
        }
    }
}
