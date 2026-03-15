# EvePI

A Rust MCP server that exposes EVE Online Planetary Interaction (PI) data as tools, backing a suite of VS Code Copilot agents for planning and managing PI across multiple accounts and characters.

## Table of Contents

- [Architecture](#architecture)
- [Prerequisites](#prerequisites)
- [Building](#building)
- [First-Time Setup](#first-time-setup)
- [GitHub Authentication](#github-authentication)
- [Running the MCP Server in VS Code](#running-the-mcp-server-in-vs-code)
- [Verifying MCP in VS Code](#verifying-mcp-in-vs-code)
- [VS Code Copilot Agents](#vs-code-copilot-agents)
- [MCP Tools Reference](#mcp-tools-reference)
- [Project Structure](#project-structure)
- [Development](#development)
- [Troubleshooting](#troubleshooting)

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
export EVE_HOME_SYSTEM_ID="31000001"   # your wormhole system ID (optional but recommended)
```

`EVE_CLIENT_ID` is required for both `enroll` and the MCP server itself. `EVE_HOME_SYSTEM_ID` is optional, but recommended so `travel_needed` is still meaningful before the first sync.

The binary does **not** load `.env` files by itself. Set these in your shell profile (`~/.bashrc`, `~/.profile`, etc.), load them via a tool like `direnv`, or pass them explicitly in `.vscode/mcp.json`.

> **WSL users:** Add them inside WSL, not Windows PowerShell. VS Code launches the MCP server with the environment it sees at startup.

### 3. Enrol characters

**Preferred method — from Copilot chat (once the MCP server is running):**

```text
#evepi-server start_eve_auth
```

The tool returns an EVE SSO URL. Open it in your browser, authorise the character, and the server catches the callback automatically on port `7878`. Run `sync_characters` afterwards to pull colony data. Repeat for each character, optionally passing an `account` label to group characters together.

**Alternative — CLI `enroll` subcommand (pre-MCP-server setup):**

```bash
cargo build --release

# Enrol a character; prompted for an account label interactively
EVE_CLIENT_ID="your_eve_client_id" ./target/release/evepi-server enroll

# Or supply the account label up front
EVE_CLIENT_ID="your_eve_client_id" ./target/release/evepi-server enroll --account "Main Account"
```

> **Token storage:** EvePI probes the OS keychain and automatically falls back to an `age`-encrypted file at `~/.config/evepi/tokens.age` when no secret service is available (the common case in WSL2).

---

## GitHub Authentication

GitHub auth is optional and only needed for the `analyse_colony` tool. If you skip this step, the MCP server still works; only AI-assisted colony analysis is unavailable.

### 1. Create a GitHub OAuth app

1. Go to [github.com/settings/developers](https://github.com/settings/developers) → **New OAuth App**.
2. Set the callback URL to `http://localhost:7879/callback`.
3. Note the **Client ID** and **Client Secret**.

### 2. Set GitHub environment variables

```bash
export GITHUB_CLIENT_ID="your_github_client_id"
export GITHUB_CLIENT_SECRET="your_github_client_secret"
```

### 3. Run the GitHub auth flow

The command starts a local listener on port `7879`, so that port must be free.

```bash
./target/release/evepi-server github-auth
```

The command will print a GitHub authorization URL, wait for the callback, then store the resulting token under the fixed key `github_token`.

---

## Running the MCP Server in VS Code

The server is designed to be launched by VS Code over stdio, not run manually in a terminal. Configure it in `.vscode/mcp.json`:

```json
{
  "servers": {
    "evepi-server": {
      "type": "stdio",
      "command": "${workspaceFolder}/target/release/evepi-server",
      "autoStart": true,
      "env": {
        "EVE_CLIENT_ID": "${env:EVE_CLIENT_ID}",
        "EVE_HOME_SYSTEM_ID": "${env:EVE_HOME_SYSTEM_ID}"
      }
    }
  }
}
```

Recommended workflow:

1. Build the binary first:

   ```bash
   cargo build --release
   ```

2. Save the `.vscode/mcp.json` file shown above.
3. Open the repository in VS Code with GitHub Copilot enabled.
4. Let VS Code auto-start the MCP server, or start/restart it from VS Code's MCP UI if needed.
5. Rebuild with `cargo build --release` any time you change Rust code, then restart the MCP server so VS Code picks up the new binary.

If VS Code does not inherit your shell environment, replace the `${env:...}` placeholders with literal values temporarily or restart VS Code from a shell where the variables are already exported.

## Verifying MCP in VS Code

Once the server is running, verify it from Copilot chat before relying on the bundled agents.

Good smoke tests:

```text
#evepi-server sync my characters and list my colonies
@pi-ops what needs doing?
@pi-analyst analyse the full P1→P2 supply chain for Alice
```

Expected behavior:

1. `#evepi-server` prompts should return tool-backed data instead of configuration errors.
2. `@pi-ops` should call `sync_characters`, then summarize urgent resets.
3. `@pi-analyst` should be able to inspect colony layouts; `analyse_colony` will only work after `github-auth`.

---

## VS Code Copilot Agents

Two specialised agents are defined in `.github/agents/`:

### `@pi-ops` — Daily operations

Use for the daily "what do I need to do?" workflow.

**Example prompts:**
```
@pi-ops what's expiring in the next 12 hours?
@pi-ops who should reset first today?
@pi-ops what does Alice need to do?
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
@pi-analyst why is Alice's Lava I underperforming?
@pi-analyst analyse the full P1→P2 supply chain for Alice
@pi-analyst is there a bottleneck in our P2 production?
```

The agent will:
1. List colonies and map their roles (extractor / processor / advanced processor)
2. Fetch full pin/route layouts for planets of interest
3. Identify idle pins, missing routes, and supply mismatches
4. Optionally call `analyse_colony` for AI-assisted analysis (requires GitHub auth — see below)

### GitHub Copilot AI analysis

The `analyse_colony` tool sends colony data to the GitHub Copilot chat completions API for deeper AI analysis. This requires a GitHub token with the `copilot` scope, stored under the key `github_token` in the token store.

Run `./target/release/evepi-server github-auth` first if you want this tool to work.

> **Important:** AI analysis is always explicit and user-initiated. The server never calls the Copilot API automatically or on a background schedule.

---

## MCP Tools Reference

All tools are callable from any Copilot agent that has `evepi-server` in its `tools` list, or directly from Copilot chat using `#evepi-server`.

| Tool | Parameters | Description |
|---|---|---|
| `start_eve_auth` | `account?` | Starts the EVE SSO flow from chat. Returns a URL to open in your browser; the callback is caught automatically by the background server on port 7878. Pass an optional account label to group characters. Run `sync_characters` afterwards. |
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
export EVE_HOME_SYSTEM_ID=31000001   # replace with your wormhole system ID
```

To find your wormhole system ID, look it up in-game or from an [ESI universe/systems search](https://esi.evetech.net/ui/#/Universe/get_universe_systems). The ID is always a large integer (e.g. `31000001`).

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

---

## Troubleshooting

### `EVE_CLIENT_ID environment variable not set`

The MCP server now checks this at startup. Set `EVE_CLIENT_ID` in your shell and/or `.vscode/mcp.json`, then restart the server from VS Code.

### `failed to bind port 7878` or `failed to bind port 7879`

Another process is already using the local callback port needed for `enroll` or `github-auth`. Stop the conflicting process or free the port, then rerun the command.

### Token storage fails on WSL or Linux

EvePI probes the OS keychain at startup and automatically falls back to an `age`-encrypted file at `~/.config/evepi/tokens.age` if the keychain is not functional (common in WSL2). If you see token-storage errors, check that `~/.config/evepi/` is writable.

### `GitHub token not found`

You tried to use `analyse_colony` before running `./target/release/evepi-server github-auth`, or the token was not stored successfully.

### VS Code is still using an old binary

Run `cargo build --release` again and restart the MCP server from VS Code. The server command points at `target/release/evepi-server`, so rebuilds are not picked up until you restart it.
