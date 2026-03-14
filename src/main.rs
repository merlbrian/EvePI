// Scaffolded modules contain items not yet wired to a calling path.
// Remove these allowances progressively as modules are connected.
#![allow(dead_code, unused_imports)]

mod auth;
mod copilot;
mod db;
mod esi;
mod mcp;
mod pi;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "evepi-server", about = "EvePI MCP server and character management")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Start the MCP server over stdio (default when no subcommand is given)
    Serve,
    /// Enrol a new EVE character via EVE SSO
    Enroll {
        /// Account label to group this character under
        #[arg(short, long)]
        account: Option<String>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Serve) {
        Command::Serve => mcp::serve().await,
        Command::Enroll { account } => auth::run_enroll(account).await,
    }
}
