//! MCP server module.
//!
//! Exposes all PI data and actions as MCP tools over stdio transport.
//! VS Code connects to this server by launching the binary; all communication
//! is JSON-RPC over stdin/stdout.

mod tools;

use anyhow::Context as _;
use rmcp::{ServiceExt, transport::stdio};

use crate::auth::TokenStore;
use crate::db::Db;
use crate::esi::EsiClient;

/// Start the MCP server and block until the client disconnects.
pub async fn serve() -> anyhow::Result<()> {
    let db = Db::open().await.context("failed to open database")?;
    let esi = EsiClient::new().context("failed to create ESI client")?;
    let store = TokenStore::new().context("failed to open token store")?;

    let home_system_id = std::env::var("EVE_HOME_SYSTEM_ID")
        .ok()
        .and_then(|v| v.parse::<i64>().ok());

    let service = tools::EvepiService::new(db, esi, store, home_system_id);

    let transport = stdio();
    service
        .serve(transport)
        .await
        .context("MCP server error")?
        .waiting()
        .await
        .context("MCP server wait error")?;

    Ok(())
}
