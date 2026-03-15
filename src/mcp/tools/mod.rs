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
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{Implementation, ServerCapabilities, ServerInfo},
    schemars, tool, tool_handler, tool_router,
};
use serde::Deserialize;

use crate::auth::TokenStore;
use crate::db::Db;
use crate::esi::EsiClient;

// --- Parameter structs ---

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct ListColoniesParams {
    /// Optional EVE character ID to filter by
    character_id: Option<i64>,
    /// Optional internal account ID to filter by
    account_id: Option<i64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetColonyLayoutParams {
    /// EVE character ID
    character_id: i64,
    /// EVE planet ID
    planet_id: i64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct GetExpiringProgramsParams {
    /// Hours ahead to look for expiring programs (default 12)
    hours: Option<i64>,
    /// Optional EVE character ID to filter by
    character_id: Option<i64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SuggestScheduleParams {
    /// Optional EVE character ID to restrict to one character
    character_id: Option<i64>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct SetPocoTaxParams {
    /// EVE planet ID
    planet_id: i64,
    /// Tax rate as a decimal fraction, e.g. 0.1 for 10%
    tax_rate: f64,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
struct AnalyseColonyParams {
    /// EVE character ID
    character_id: i64,
    /// EVE planet ID
    planet_id: i64,
}

// --- Service ---

/// The MCP service that handles all tool calls.
#[derive(Debug, Clone)]
pub struct EvepiService {
    pub db: Db,
    pub esi: EsiClient,
    pub store: TokenStore,
    pub client_id: String,
    /// Optional home solar system ID, read from `EVE_HOME_SYSTEM_ID`.
    /// Used as a fallback when a character's current location is unknown
    /// (i.e. `sync_characters` has not been run yet for that character).
    pub home_system_id: Option<i64>,
    tool_router: ToolRouter<Self>,
}

impl EvepiService {
    pub fn new(
        db: Db,
        esi: EsiClient,
        store: TokenStore,
        client_id: String,
        home_system_id: Option<i64>,
    ) -> Self {
        Self {
            db,
            esi,
            store,
            client_id,
            home_system_id,
            tool_router: Self::tool_router(),
        }
    }
}

#[tool_router]
impl EvepiService {
    #[tool(
        description = "Sync PI data from ESI for all enrolled characters. Call this first to get fresh data."
    )]
    async fn sync_characters(&self) -> String {
        match sync_characters::handle(self).await {
            Ok(v) => v.to_string(),
            Err(e) => format!("{{\"error\": \"{e}\"}}"),
        }
    }

    #[tool(
        description = "List all colonies. Pass character_id or account_id to filter. Returns planet label, role, soonest extractor expiry, and travel_needed flag."
    )]
    async fn list_colonies(
        &self,
        Parameters(ListColoniesParams {
            character_id,
            account_id,
        }): Parameters<ListColoniesParams>,
    ) -> String {
        match list_colonies::handle(self, character_id, account_id).await {
            Ok(v) => v.to_string(),
            Err(e) => format!("{{\"error\": \"{e}\"}}"),
        }
    }

    #[tool(description = "Get the full pin/route layout for a single colony.")]
    async fn get_colony_layout(
        &self,
        Parameters(GetColonyLayoutParams {
            character_id,
            planet_id,
        }): Parameters<GetColonyLayoutParams>,
    ) -> String {
        match get_colony_layout::handle(self, character_id, planet_id).await {
            Ok(v) => v.to_string(),
            Err(e) => format!("{{\"error\": \"{e}\"}}"),
        }
    }

    #[tool(
        description = "List extractor programs expiring within the given number of hours (default 12). Includes planet label and travel_needed flag."
    )]
    async fn get_expiring_programs(
        &self,
        Parameters(GetExpiringProgramsParams {
            hours,
            character_id,
        }): Parameters<GetExpiringProgramsParams>,
    ) -> String {
        match get_expiring_programs::handle(self, hours.unwrap_or(12), character_id).await {
            Ok(v) => v.to_string(),
            Err(e) => format!("{{\"error\": \"{e}\"}}"),
        }
    }

    #[tool(
        description = "Suggest an ordered reset schedule across all characters, prioritising most urgent expiries and characters already in the colony system."
    )]
    async fn suggest_schedule(
        &self,
        Parameters(SuggestScheduleParams { character_id }): Parameters<SuggestScheduleParams>,
    ) -> String {
        match suggest_schedule::handle(self, character_id).await {
            Ok(v) => v.to_string(),
            Err(e) => format!("{{\"error\": \"{e}\"}}"),
        }
    }

    #[tool(description = "Manually set the POCO tax rate (0.0-1.0) for a planet. Defaults to 0.0.")]
    async fn set_poco_tax(
        &self,
        Parameters(SetPocoTaxParams {
            planet_id,
            tax_rate,
        }): Parameters<SetPocoTaxParams>,
    ) -> String {
        match set_poco_tax::handle(self, planet_id, tax_rate).await {
            Ok(v) => v.to_string(),
            Err(e) => format!("{{\"error\": \"{e}\"}}"),
        }
    }

    #[tool(
        description = "Run an AI efficiency analysis on one colony using GitHub Copilot. Requires GitHub authentication. Always explicit - never called automatically."
    )]
    async fn analyse_colony(
        &self,
        Parameters(AnalyseColonyParams {
            character_id,
            planet_id,
        }): Parameters<AnalyseColonyParams>,
    ) -> String {
        match analyse_colony::handle(self, character_id, planet_id).await {
            Ok(v) => v.to_string(),
            Err(e) => format!("{{\"error\": \"{e}\"}}"),
        }
    }
}

#[tool_handler]
impl ServerHandler for EvepiService {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_server_info(
            Implementation::new("evepi-server", env!("CARGO_PKG_VERSION")),
        )
    }
}
