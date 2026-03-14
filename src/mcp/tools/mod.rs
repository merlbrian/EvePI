//! MCP tool definitions and handler implementations.

mod analyse_colony;
mod get_colony_layout;
mod get_expiring_programs;
mod list_colonies;
mod set_poco_tax;
mod suggest_schedule;
mod sync_characters;

use rmcp::{
    ServerHandler,
    model::{Implementation, ServerCapabilities, ServerInfo},
    tool,
};

use crate::auth::TokenStore;
use crate::db::Db;
use crate::esi::EsiClient;

/// The MCP service that handles all tool calls.
#[derive(Debug, Clone)]
pub struct EvepiService {
    pub db: Db,
    pub esi: EsiClient,
    pub store: TokenStore,
}

impl EvepiService {
    pub fn new(db: Db, esi: EsiClient, store: TokenStore) -> Self {
        Self { db, esi, store }
    }
}

#[tool(tool_box)]
impl EvepiService {
    /// Sync all colonies and current location for all enrolled characters.
    #[tool(
        description = "Sync PI data from ESI for all enrolled characters. Call this first to get fresh data."
    )]
    async fn sync_characters(&self) -> String {
        match sync_characters::handle(self).await {
            Ok(v) => v.to_string(),
            Err(e) => format!("{{\"error\": \"{e}\"}}"),
        }
    }

    /// List colonies across characters with optional filters.
    #[tool(
        description = "List all colonies. Pass character_id or account_id to filter. Returns planet label, role, soonest extractor expiry, and travel_needed flag."
    )]
    async fn list_colonies(
        &self,
        #[tool(param)]
        #[schemars(description = "Optional EVE character ID to filter by")]
        character_id: Option<i64>,
        #[tool(param)]
        #[schemars(description = "Optional internal account ID to filter by")]
        account_id: Option<i64>,
    ) -> String {
        match list_colonies::handle(self, character_id, account_id).await {
            Ok(v) => v.to_string(),
            Err(e) => format!("{{\"error\": \"{e}\"}}"),
        }
    }

    /// Get the full pin/route/schematic layout for one colony.
    #[tool(description = "Get the full pin/route layout for a single colony.")]
    async fn get_colony_layout(
        &self,
        #[tool(param)]
        #[schemars(description = "EVE character ID")]
        character_id: i64,
        #[tool(param)]
        #[schemars(description = "EVE planet ID")]
        planet_id: i64,
    ) -> String {
        match get_colony_layout::handle(self, character_id, planet_id).await {
            Ok(v) => v.to_string(),
            Err(e) => format!("{{\"error\": \"{e}\"}}"),
        }
    }

    /// List extractor programs expiring within N hours.
    #[tool(
        description = "List extractor programs expiring within the given number of hours (default 12). Includes planet label and travel_needed flag."
    )]
    async fn get_expiring_programs(
        &self,
        #[tool(param)]
        #[schemars(description = "Hours ahead to look for expiring programs (default 12)")]
        hours: Option<i64>,
        #[tool(param)]
        #[schemars(description = "Optional EVE character ID to filter by")]
        character_id: Option<i64>,
    ) -> String {
        match get_expiring_programs::handle(self, hours.unwrap_or(12), character_id).await {
            Ok(v) => v.to_string(),
            Err(e) => format!("{{\"error\": \"{e}\"}}"),
        }
    }

    /// Suggest a ranked reset schedule across all characters.
    #[tool(
        description = "Suggest an ordered reset schedule across all characters, prioritising most urgent expiries and characters already in the colony system."
    )]
    async fn suggest_schedule(
        &self,
        #[tool(param)]
        #[schemars(description = "Optional EVE character ID to restrict to one character")]
        character_id: Option<i64>,
    ) -> String {
        match suggest_schedule::handle(self, character_id).await {
            Ok(v) => v.to_string(),
            Err(e) => format!("{{\"error\": \"{e}\"}}"),
        }
    }

    /// Set the POCO tax rate for a planet manually.
    #[tool(description = "Manually set the POCO tax rate (0.0-1.0) for a planet. Defaults to 0.0.")]
    async fn set_poco_tax(
        &self,
        #[tool(param)]
        #[schemars(description = "EVE planet ID")]
        planet_id: i64,
        #[tool(param)]
        #[schemars(description = "Tax rate as a decimal fraction, e.g. 0.1 for 10%")]
        tax_rate: f64,
    ) -> String {
        match set_poco_tax::handle(self, planet_id, tax_rate).await {
            Ok(v) => v.to_string(),
            Err(e) => format!("{{\"error\": \"{e}\"}}"),
        }
    }

    /// Run a Copilot AI analysis on a single colony (explicit; requires GitHub auth).
    #[tool(
        description = "Run an AI efficiency analysis on one colony using GitHub Copilot. Requires GitHub authentication. Always explicit - never called automatically."
    )]
    async fn analyse_colony(
        &self,
        #[tool(param)]
        #[schemars(description = "EVE character ID")]
        character_id: i64,
        #[tool(param)]
        #[schemars(description = "EVE planet ID")]
        planet_id: i64,
    ) -> String {
        match analyse_colony::handle(self, character_id, planet_id).await {
            Ok(v) => v.to_string(),
            Err(e) => format!("{{\"error\": \"{e}\"}}"),
        }
    }
}

#[tool(tool_box)]
impl ServerHandler for EvepiService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: Implementation {
                name: "evepi-server".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}
