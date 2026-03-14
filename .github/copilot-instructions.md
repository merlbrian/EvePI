# EvePI

A Rust MCP server that exposes EVE Online Planetary Interaction (PI) data as tools, backing a suite of VS Code Copilot agents for planning and managing PI across multiple accounts and characters. Uses the [EVE ESI API](https://esi.evetech.net/ui/).

## Architecture

EvePI follows a **hybrid MCP + Copilot agents** pattern:

- **Rust MCP server** (`evepi-server`): The core binary. Handles EVE SSO auth, ESI sync, SQLite persistence, and exposes all data and actions as MCP tools. VS Code Copilot agents call these tools to answer questions and take actions.
- **VS Code agents** (`.github/agents/`): Specialised Copilot agents (e.g. `pi-planner`, `colony-analyst`) that use the MCP tools to provide PI planning assistance in chat. No app UI to build or maintain.
- **ESI integration**: All EVE API calls go through the ESI REST API. Auth is OAuth2 (EVE SSO).
- **Multi-account / multi-character**: The data model supports N accounts × M characters; queries and plans should never assume a single character.
- **PI domain**: Core entities are `Colony`, `Planet`, `ExtractorHead`, `Program`, `Route`, and `Product`. Use EVE's official terminology throughout.
- **Planet display format**: Always identify planets as `{PlanetType} {RomanNumeral}` within a character/system context (e.g. "Gas III", "Lava I"). This is the user's established convention. `Colony::display_label()` produces this string. Never use raw planet IDs in user-facing output.
- **Fleet scale**: ~9 characters across 5+ accounts, up to 7 planets per character. All list tools must support `character_id`/`account_id` filters — never return all colonies for all characters unsolicited.
- **Cycle standard**: The default PI cycle is 6 days. Expiry times and schedule recommendations should be expressed relative to this baseline.
- **PI chain design**: The fleet runs a three-tier chain: (1) P0→P1 extractor planets extract raw resources; (2) P1→P2 processor planets import P1 from extractors, process into P2; (3) P2→P3/P4 advanced processor planets refine further. Inter-planet hauling is a physical trip the character makes — they must be in the same solar system as their colonies. Planet role (extractor / processor / advanced_processor) is derived from pin types on that colony.
- **Travel awareness**: All colonies currently live in one wormhole system. The travel flag on any tool response is: is the character currently in that system? Store `solar_system_id` per colony so multi-system support is automatic when needed.
- **Copilot AI assistant**: The MCP server includes a `copilot` module that wraps the GitHub Copilot chat completions API for in-server AI analysis (e.g. efficiency scoring, schedule recommendations). All AI calls go through this module; the user's GitHub token (scope: `copilot`) is used for auth — never bundle a hardcoded key.

## Copilot Integration

- **Endpoint**: `https://api.githubcopilot.com/chat/completions` — OpenAI-compatible chat completion format.
- **Auth**: Bearer token from the user's GitHub OAuth flow (scope: `copilot`). Store and refresh via the `auth` module alongside EVE SSO tokens.
- **Token storage**: Use the `keyring` crate as the primary store (works natively on Windows via Credential Manager). Fall back to an `age`-encrypted file at `~/.config/evepi/tokens.age` when no OS secret service is available (the common case in WSL). The `auth` module owns all keychain/file reads and writes — no other module touches tokens directly. Never write tokens to disk unencrypted.
- **Module**: `src/copilot/` — owns prompt construction, request/response types, and streaming. Never scatter raw HTTP calls to the Copilot API outside this module.
- **Prompts**: System prompt always includes current colony snapshot (planet type, extractor programs, storage levels) so the model has full context. Keep prompts in `src/copilot/prompts/` as separate `.txt` / `.md` files — not hardcoded strings.
- **Streaming**: Use SSE streaming (`stream: true`) for user-facing responses so output appears incrementally.
- **No PII in prompts**: Only send game data (character IDs, planet types, resource types, quantities). Never include account credentials, real names, or EVE SSO tokens in prompts.
- **Explicit only**: AI assistance is always user-initiated (e.g. an explicit command or UI action). Never call the Copilot API proactively or on a background schedule.
- **Fallback**: All Copilot calls are optional enhancements. The app must remain fully functional if the Copilot API is unreachable or the user has not authenticated with GitHub.

## Module Structure

```
src/
  main.rs          # MCP server entry point (tokio::main)
  auth/            # EVE SSO + GitHub OAuth flows, token storage
  esi/             # ESI HTTP client, rate-limit handling, PI endpoint models
  pi/              # Domain types: Colony, Planet, ExtractorHead, Program, Route, Product
  db/              # SQLite persistence via sqlx (migrations in db/migrations/)
  mcp/             # MCP tool definitions and handler dispatch
  copilot/         # GitHub Copilot API client, prompt construction, SSE streaming
    prompts/       # System/user prompt templates as .md files
.github/
  agents/          # VS Code Copilot agent definitions (.agent.md)
  prompts/         # Reusable prompt files (.prompt.md)
```

## MCP Tools (planned)

- `sync_characters` — fetch all planets/colonies + current location for all authenticated characters
- `list_colonies` — colony snapshot filterable by character/account; includes planet role and `travel_needed` flag
- `get_colony_layout` — full pin/route/schematic tree for one colony
- `get_expiring_programs` — extractor programs expiring within N hours, with character location context
- `suggest_schedule` — ranked reset order across characters, factoring expiry urgency + who is in the hole
- `analyse_colony` — invoke Copilot analysis on a single colony's efficiency (explicit; requires GitHub auth)
- `set_poco_tax` — manually set POCO tax rate per planet (default 0%)

## Build and Test

```bash
cargo build          # compile
cargo test           # run all tests
cargo clippy         # lint (all warnings must be resolved)
cargo fmt            # format (enforced, no diffs allowed)
```

Before committing, always run `cargo clippy -- -D warnings` and `cargo fmt --check`.

## Conventions

- **Error handling**: Use `thiserror` for library/domain errors, `anyhow` for application-level error propagation. Never `.unwrap()` in non-test code.
- **Async runtime**: `tokio` (multi-thread). Mark async entry points with `#[tokio::main]`.
- **HTTP client**: `reqwest` with connection pooling; reuse a single `Client` instance.
- **Serialization**: `serde` + `serde_json`. ESI response structs derive `Deserialize`; domain types derive `Serialize` + `Deserialize` only when needed.
- **ESI rate-limits**: Respect the `X-ESI-Error-Limit-Remain` / `X-ESI-Error-Limit-Reset` headers. Back off when the error limit drops below 20.
- **Naming**: snake_case for variables/functions, PascalCase for types, SCREAMING_SNAKE_CASE for constants. Module names mirror domain concepts (e.g., `esi`, `pi`, `auth`, `db`, `copilot`).
- **Tests**: Unit-test pure domain logic with small inline `#[test]` blocks. Integration tests that call ESI live go in `tests/` and are gated with `#[cfg(feature = "integration")]`.
- **No `unwrap` rule**: Use `expect("reason")` in tests; use `?` with meaningful context in production code.

## Key References

- [ESI API docs](https://esi.evetech.net/ui/) — all PI endpoints live under `/characters/{character_id}/planets/`
- [EVE SSO OAuth2](https://developers.eveonline.com/blog/article/sso-to-authenticated-calls) — required for character-scoped endpoints
- [Rust API guidelines](https://rust-lang.github.io/api-guidelines/) — follow for public-facing types
