// Scaffolded modules contain items not yet wired to a calling path.
// Remove these allowances progressively as modules are connected.
#![allow(dead_code, unused_imports)]

mod auth;
mod copilot;
mod db;
mod esi;
mod mcp;
mod pi;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    mcp::serve().await
}
