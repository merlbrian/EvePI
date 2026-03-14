//! PI domain types.
//!
//! These are the canonical in-memory representations used throughout the app.
//! ESI response structs (in `esi::models`) are mapped into these types before
//! being stored or passed to MCP tools.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Identifiers
// ---------------------------------------------------------------------------

pub type CharacterId = i64;
pub type PlanetId = i64;
pub type PinId = i64;
pub type SolarSystemId = i64;
pub type TypeId = i64;

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Colony {
    pub character_id: CharacterId,
    pub planet_id: PlanetId,
    pub planet_type: PlanetType,
    /// 1-based index of this planet type within the solar system.
    /// e.g. the second Gas planet in the system has planet_index = 2.
    /// This drives the "Gas III" style display label.
    pub planet_index: u8,
    pub solar_system_id: SolarSystemId,
    pub upgrade_level: u8,
    pub num_pins: u32,
    pub last_update: DateTime<Utc>,
}

impl Colony {
    /// Human-readable label in the style the user already uses: "Gas III", "Lava I".
    pub fn display_label(&self) -> String {
        format!("{} {}", self.planet_type, to_roman(self.planet_index))
    }
}

/// The role of a colony in the PI production chain, derived from its pin types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ColonyRole {
    /// Has extractor pins only — produces P1 from P0 raw resources.
    Extractor,
    /// Has factory pins processing P1→P2. May also have extractors.
    Processor,
    /// Has factory pins processing P2→P3 or P3→P4.
    AdvancedProcessor,
    /// No active programs; planet is idle.
    Idle,
}

impl ColonyRole {
    /// Infer role from a slice of pins on the colony.
    pub fn from_pins(pins: &[Pin]) -> Self {
        let has_extractor = pins.iter().any(|p| matches!(p, Pin::Extractor(_)));
        let factory_schematics: Vec<Option<u32>> = pins
            .iter()
            .filter_map(|p| {
                if let Pin::Factory(f) = p {
                    Some(f.schematic_id)
                } else {
                    None
                }
            })
            .collect();

        if factory_schematics.is_empty() {
            if has_extractor {
                return ColonyRole::Extractor;
            }
            return ColonyRole::Idle;
        }

        // Schematic IDs 15–25 are P2 outputs; 26–118 are P3/P4 outputs.
        // We use a simple heuristic: treat any schematic with id >= 26 as advanced.
        let is_advanced = factory_schematics
            .iter()
            .any(|id| id.map(|s| s >= 26).unwrap_or(false));

        if is_advanced {
            ColonyRole::AdvancedProcessor
        } else {
            ColonyRole::Processor
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanetType {
    Temperate,
    Barren,
    Oceanic,
    Ice,
    Gas,
    Lava,
    Storm,
    Plasma,
}

impl std::fmt::Display for PlanetType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            PlanetType::Temperate => "Temperate",
            PlanetType::Barren => "Barren",
            PlanetType::Oceanic => "Oceanic",
            PlanetType::Ice => "Ice",
            PlanetType::Gas => "Gas",
            PlanetType::Lava => "Lava",
            PlanetType::Storm => "Storm",
            PlanetType::Plasma => "Plasma",
        };
        f.write_str(s)
    }
}

/// Convert a small positive integer to a Roman numeral string (I–XII).
pub fn to_roman(n: u8) -> &'static str {
    match n {
        1 => "I",
        2 => "II",
        3 => "III",
        4 => "IV",
        5 => "V",
        6 => "VI",
        7 => "VII",
        8 => "VIII",
        9 => "IX",
        10 => "X",
        11 => "XI",
        12 => "XII",
        _ => "?",
    }
}

/// A single installation on a planet. The variant determines what it does.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Pin {
    Extractor(ExtractorPin),
    Factory(FactoryPin),
    Launchpad(BasicPin),
    Storage(BasicPin),
    CommandCenter(BasicPin),
}

impl Pin {
    pub fn pin_id(&self) -> PinId {
        match self {
            Pin::Extractor(p) => p.base.pin_id,
            Pin::Factory(p) => p.base.pin_id,
            Pin::Launchpad(p) => p.pin_id,
            Pin::Storage(p) => p.pin_id,
            Pin::CommandCenter(p) => p.pin_id,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasicPin {
    pub pin_id: PinId,
    pub type_id: TypeId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractorPin {
    #[serde(flatten)]
    pub base: BasicPin,
    pub product_type_id: Option<TypeId>,
    pub cycle_time: Option<u32>,
    pub head_radius: Option<f32>,
    pub heads: Vec<ExtractorHead>,
    pub expiry_time: Option<DateTime<Utc>>,
    pub install_time: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractorHead {
    pub head_id: u32,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactoryPin {
    #[serde(flatten)]
    pub base: BasicPin,
    pub schematic_id: Option<u32>,
}

/// A route connecting two pins for a given resource type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Route {
    pub route_id: i64,
    pub source_pin_id: PinId,
    pub destination_pin_id: PinId,
    pub content_type_id: TypeId,
    pub quantity: f64,
}

/// Schematic (factory blueprint): inputs/outputs and cycle time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schematic {
    pub schematic_id: u32,
    pub schematic_name: String,
    pub cycle_time: u32,
    pub inputs: Vec<SchematicIO>,
    pub output: SchematicIO,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchematicIO {
    pub type_id: TypeId,
    pub quantity: u32,
}

/// Customs office (POCO) for a planet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomsOffice {
    pub planet_id: PlanetId,
    /// Tax rate as a fraction (0.0–1.0). Defaults to 0.0 if not set.
    pub tax_rate: f64,
    /// True when the value was pulled from ESI; false when manually entered.
    pub from_esi: bool,
}

/// A character's current location.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterLocation {
    pub character_id: CharacterId,
    pub solar_system_id: SolarSystemId,
    pub solar_system_name: Option<String>,
}

/// Cached solar system info (handles wormhole system IDs too).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SolarSystem {
    pub solar_system_id: SolarSystemId,
    pub solar_system_name: String,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_label_gas_iii() {
        let colony = Colony {
            character_id: 1,
            planet_id: 1,
            planet_type: PlanetType::Gas,
            planet_index: 3,
            solar_system_id: 1,
            upgrade_level: 4,
            num_pins: 6,
            last_update: chrono::Utc::now(),
        };
        assert_eq!(colony.display_label(), "Gas III");
    }

    #[test]
    fn display_label_lava_i() {
        let colony = Colony {
            character_id: 1,
            planet_id: 2,
            planet_type: PlanetType::Lava,
            planet_index: 1,
            solar_system_id: 1,
            upgrade_level: 4,
            num_pins: 6,
            last_update: chrono::Utc::now(),
        };
        assert_eq!(colony.display_label(), "Lava I");
    }

    #[test]
    fn pin_id_extractor() {
        let pin = Pin::Extractor(ExtractorPin {
            base: BasicPin {
                pin_id: 42,
                type_id: 1,
            },
            product_type_id: None,
            cycle_time: None,
            head_radius: None,
            heads: vec![],
            expiry_time: None,
            install_time: None,
        });
        assert_eq!(pin.pin_id(), 42);
    }

    #[test]
    fn customs_office_default_tax() {
        let poco = CustomsOffice {
            planet_id: 1,
            tax_rate: 0.0,
            from_esi: false,
        };
        assert_eq!(poco.tax_rate, 0.0);
    }
}
