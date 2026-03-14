# EvePI

A Rust MCP server that exposes EVE Online Planetary Interaction (PI) data as tools, backing a suite of VS Code Copilot agents for planning and managing PI across multiple accounts and characters.

## Table of Contents

- [Architecture](#architecture)
- [Prerequisites](#prerequisites)
- [Building](#building)
- [First-Time Setup](#first-time-setup)
- [Running the MCP Server](#running-the-mcp-server)
- [VS Code Copilot Agents](#vs-code-copilot-agents)
- [MCP Tools Reference](#mcp-tools-reference)
- [Project Structure](#project-structure)
- [Development](#development)

---

## Architecture

```
VS Code Copilot chat
       │
       │ MCP (stdio)
       ▼
evepi-server  ──── EVE ESI API (https://esi.evetech.net)
       │
       ├── SQLite  (~/.config/evepi/evepi.db)
       └── Token store  (OS keychain → age-encrypted file fallback)
```

The server speaks the **Model Context Protocol** over stdio. VS Code launches it as a subprocess and Copilot agents call its tools to answer questions and take actions. There is no separate UI — everything is driven from the Copilot chat panel.

---

## Prerequisites

| Requirement | Notes |
|---|---|
| Rust stable (≥ 1.87) | `rustup toolchain install stable` |
| VS Code with GitHub Copilot | Chat panel must be enabled |
| EVE developer application | Create at [developers.eveonline.com](https://developers.eveonline.com) |
| `pkg-config` + `libssl-dev` | On Ubuntu/WSL: `sudo apt-get install pkg-config libssl-dev` |

---

## Building

```bash
git clone <repo-url>
cd evepi
cargo build --release
```

The compiled binary is at `target/release/evepi-server`.

For development:

```bash
cargo build            # debug build
cargo check            # type-check only (fast)
cargo clippy -- -D warnings   # lint (must be clean before committing)
cargo fmt --check      # formatting check (must be clean before committing)
cargo test             # unit tests
```

---

## First-Time Setup

### 1. Create an EVE developer application

1. Go to [developers.eveonline.com](https://developers.eveonline.com) → **Manage Applications → Create New Application**.
2. Set the callback URL to `http://localhost:7878/callback`.
3. Add the following scopes:
   - `esi-planets.manage_planets.v1`
   - `esi-location.read_location.v1`
4. Note the **Client ID**.

### 2. Set environment variables

```bash
export EVE_CLIENT_ID="your_eve_client_id"
export EVE_HOME_SYSTEM_ID="31002229"   # your wormhole system ID (optional but recommended)
```

Add this to your shell profile (`~/.bashrc`, `~/.profile`, etc.) or to `.env` in the repo root (it is gitignored).

> **WSL users:** Add to `~/.bashrc` inside WSL. The MCP server inherits the environment VS Code passes to its terminal, so make sure VS Code is started from a shell where these are set.

### 3. Enrol characters

Run the `enroll` subcommand once per character. It spins up a local HTTP listener on port 7878 and guides you through the EVE SSO flow in your browser.

```bash
# Build first (or use `cargo run --`)
cargo build --release

# Enrol a character; prompted for an account label interactively
EVE_CLIENT_ID="your_eve_client_id" ./target/release/evepi-server enroll

# Or supply the account label up front
EVE_CLIENT_ID="your_eve_client_id" ./target/release/evepi-server enroll --account "Main Account"
```

The command will:
1. Print an EVE SSO authorization URL — open it in your browser.
2. Log in with the EVE character you want to add, and click **Authorize**.
3. The browser redirects to `http://localhost:7878/callback`; the server captures the code automatically.
4. Ask for an account label (press Enter to default to the character name).
5. Persist the character + tokens and print a confirmation.

Repeat for each character. The token store tries the OS keychain first (Windows Credential Manager, macOS Keychain) and falls back to `~/.config/evepi/tokens.age` for environments without a secret service daemon (e.g. WSL).

---

## Running the MCP Server

The server is designed to be launched by VS Code, not run directly. Configure it in `.vscode/settings.json`:

```json
{
  "mcp": {
    "servers": {
      "evepi-server": {
        "type": "stdio",
        "command": "${workspaceFolder}/target/release/evepi-server",
        "env": {
          "EVE_CLIENT_ID": "${env:EVE_CLIENT_ID}"
        }
      }
    }
  }
}
```

After saving, VS Code will offer to **Start** the server. Once running, the `evepi-server` tools become available to all Copilot agents.

To verify it's working, open a Copilot chat and type:

```
@pi-ops what needs doing?
```

---

## VS Code Copilot Agents

Two specialised agents are defined in `.github/agents/`:

### `@pi-ops` — Daily operations

Use for the daily "what do I need to do?" workflow.

**Example prompts:**
```
@pi-ops what's expiring in the next 12 hours?
@pi-ops who should reset first today?
@pi-ops what does Rebort need to do?
```

The agent will:
1. Call `sync_characters` to pull fresh ESI data
2. Call `get_expiring_programs` (24-hour window by default)
3. Call `suggest_schedule` to rank resets by urgency and travel cost
4. Highlight any character who needs to travel to reach their colonies

### `@pi-analyst` — Chain optimisation

Use for deeper analysis of why a planet is underperforming or how to improve the P0→P1→P2→P3/P4 chain.

**Example prompts:**
```
@pi-analyst why is Loywn's Lava I underperforming?
@pi-analyst analyse the full P1→P2 supply chain for Gemma
@pi-analyst is there a bottleneck in our P2 production?
```

The agent will:
1. List colonies and map their roles (extractor / processor / advanced processor)
2. Fetch full pin/route layouts for planets of interest
3. Identify idle pins, missing routes, and supply mismatches
4. Optionally call `analyse_colony` for AI-assisted analysis (requires GitHub auth — see below)

### GitHub Copilot AI analysis

The `analyse_colony` tool sends colony data to the GitHub Copilot chat completions API for deeper AI analysis. This requires a GitHub token with the `copilot` scope, stored under the key `github_token` in the token store.

> **Important:** AI analysis is always explicit and user-initiated. The server never calls the Copilot API automatically or on a background schedule.

---

## MCP Tools Reference

All tools are callable from any Copilot agent that has `evepi-server` in its `tools` list, or directly from Copilot chat using `#evepi-server`.

| Tool | Parameters | Description |
|---|---|---|
| `sync_characters` | — | Fetches fresh colony data and character locations from ESI for all enrolled characters. **Run this first** before any other tool. |
| `list_colonies` | `character_id?`, `account_id?` | Lists all colonies with planet label (`Gas III`, `Lava I`), upgrade level, soonest extractor expiry, and `travel_needed` flag. |
| `get_colony_layout` | `character_id`, `planet_id` | Returns the full pin/route structure for one colony. |
| `get_expiring_programs` | `hours?` (default 12), `character_id?` | Lists extractor programs expiring within the given window, ordered by expiry time. Includes travel flag. |
| `suggest_schedule` | `character_id?` | Returns a ranked reset schedule across all characters, prioritising urgent expiries and characters already in the colony system. |
| `set_poco_tax` | `planet_id`, `tax_rate` (0.0–1.0) | Manually records the POCO tax rate for a planet. Defaults to 0.0 (no tax). |
| `analyse_colony` | `character_id`, `planet_id` | Runs an AI efficiency analysis on one colony via GitHub Copilot. **Requires GitHub auth.** Never called automatically. |

### Planet labels

Planets are always identified as `{PlanetType} {RomanNumeral}` within a character/system context — for example `Gas III`, `Lava I`, `Temperate II`. The numeral reflects the planet's index within the solar system as recorded by the last sync.

### Travel flag

`travel_needed: true` means the character is **not** currently in the same solar system as their colony.

The home wormhole system is configurable via `EVE_HOME_SYSTEM_ID`. When set, any character whose current location is unknown (i.e. `sync_characters` has not yet been run) is assumed to be in the home system for the purposes of the travel flag. Once synced, the character's actual ESI location is used instead.

```bash
export EVE_HOME_SYSTEM_ID=31000001   # replace with your WH system ID
```

To find your wormhole system ID, look it up in-game or from an [ESI universe/systems search](https://esi.evetech.net/ui/#/Universe/get_universe_systems). The ID is always a large integer (e.g. `31002229`).

---

## Project Structure

```
src/
  main.rs              # Entry point — starts the MCP server
  auth/                # EVE SSO OAuth2 + GitHub OAuth, token storage
  esi/                 # ESI HTTP client, rate-limit handling, response models
  pi/                  # Domain types: Colony, Pin, Route, ExtractorHead, etc.
  db/                  # SQLite persistence (sqlx), migrations, sync logic
  mcp/                 # MCP server init and tool dispatch
    tools/             # One file per tool handler
  copilot/             # GitHub Copilot API client + prompt templates
    prompts/           # system.md, analyse_colony.md, suggest_schedule.md
db/
  migrations/          # SQL migration files (sqlx migrate)
.github/
  agents/              # pi-ops.agent.md, pi-analyst.agent.md
  prompts/             # daily-report.prompt.md
  copilot-instructions.md  # Always-on workspace instructions for all agents
```

The database is created automatically at `~/.config/evepi/evepi.db` on first run.

---

## Development

### Running tests

```bash
cargo test                              # unit tests (no ESI calls)
cargo test --features integration       # includes live ESI calls (requires auth)
```

### Code conventions

- **Errors:** `thiserror` for domain errors, `anyhow` for propagation. Never `.unwrap()` in non-test code.
- **Async:** `tokio` multi-thread. All async entry points use `#[tokio::main]`.
- **HTTP:** Single `EsiClient` instance reused across the app. ESI rate-limit headers are respected — the client backs off when the error budget drops below 20 remaining.
- **DB queries:** All queries use the `sqlx::query().bind()` runtime API (not the `query!` macro) so no `DATABASE_URL` is needed at compile time.
- **Commit checklist:** `cargo clippy -- -D warnings` and `cargo fmt --check` must both pass before committing.

### Adding a new MCP tool

1. Create `src/mcp/tools/my_tool.rs` with `pub async fn handle(...) -> anyhow::Result<Value>`.
2. Add `mod my_tool;` in `src/mcp/tools/mod.rs`.
3. Add a new `async fn` on `EvepiService` (inside the `#[tool(tool_box)]` impl block) that calls `my_tool::handle(self, ...)` and matches `Ok`/`Err` to a `String`.
4. Run `cargo clippy -- -D warnings` and `cargo fmt`.

