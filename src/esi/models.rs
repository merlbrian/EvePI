//! Raw ESI response structs.
//!
//! These derive `Deserialize` only. They are mapped to `pi::*` domain types
//! before being used elsewhere in the app.

use serde::Deserialize;

// ---------------------------------------------------------------------------
// Colonies / Planets
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct EsiColony {
    pub planet_id: i64,
    pub planet_type: String,
    pub solar_system_id: i64,
    pub upgrade_level: u8,
    pub num_pins: u32,
    pub last_update: String,
}

#[derive(Debug, Deserialize)]
pub struct EsiColonyLayout {
    pub pins: Vec<EsiPin>,
    pub routes: Vec<EsiRoute>,
    pub links: Vec<EsiLink>,
}

#[derive(Debug, Deserialize)]
pub struct EsiPin {
    pub pin_id: i64,
    pub type_id: i64,
    pub latitude: f64,
    pub longitude: f64,
    pub schematic_id: Option<u32>,
    pub extractor_details: Option<EsiExtractorDetails>,
    pub expiry_time: Option<String>,
    pub install_time: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct EsiExtractorDetails {
    pub product_type_id: Option<i64>,
    pub cycle_time: Option<u32>,
    pub head_radius: Option<f32>,
    pub heads: Vec<EsiExtractorHead>,
}

#[derive(Debug, Deserialize)]
pub struct EsiExtractorHead {
    pub head_id: u32,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Deserialize)]
pub struct EsiRoute {
    pub route_id: i64,
    pub source_pin_id: i64,
    pub destination_pin_id: i64,
    pub content_type_id: i64,
    pub quantity: f64,
}

#[derive(Debug, Deserialize)]
pub struct EsiLink {
    pub source_pin_id: i64,
    pub destination_pin_id: i64,
    pub link_level: u32,
}

// ---------------------------------------------------------------------------
// Location
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct EsiCharacterLocation {
    pub solar_system_id: i64,
    pub station_id: Option<i64>,
    pub structure_id: Option<i64>,
}

// ---------------------------------------------------------------------------
// Universe
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct EsiSolarSystem {
    pub system_id: i64,
    pub name: String,
    pub security_status: f32,
}

#[derive(Debug, Deserialize)]
pub struct EsiSchematic {
    pub schematic_name: String,
    pub cycle_time: u32,
    pub pins: Vec<EsiSchematicPin>,
}

#[derive(Debug, Deserialize)]
pub struct EsiSchematicPin {
    pub type_id: i64,
    pub quantity: u32,
    pub is_input: bool,
}

/// Minimal type info — only `type_id` and `name` are needed; other fields
/// returned by ESI are intentionally ignored.
#[derive(Debug, Deserialize)]
pub struct EsiType {
    pub type_id: i64,
    pub name: String,
}

// ---------------------------------------------------------------------------
// Customs Offices
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct EsiCustomsOffice {
    pub office_id: i64,
    pub system_id: i64,
    pub planet_id: Option<i64>,
    pub tax_rate_corp: Option<f64>,
    pub tax_rate_alliance: Option<f64>,
    pub tax_rate_standing_good: Option<f64>,
    pub tax_rate_standing_bad: Option<f64>,
    pub tax_rate_standing_neutral: Option<f64>,
}
