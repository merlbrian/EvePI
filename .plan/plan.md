# Plan: Replace GitHub Copilot with Claude API

## Goal

Remove the GitHub OAuth dependency from the analysis feature and replace the
Copilot API client with the Anthropic Claude API. No change to prompts or
colony analysis logic.

---

## Files to delete

| File | Reason |
|------|--------|
| `src/auth/github_oauth.rs` | Implements GitHub OAuth only |
| `src/copilot/client.rs` | Copilot-specific HTTP client |
| `src/copilot/mod.rs` | Re-exports CopilotClient only |

---

## Files to change

### `Cargo.toml`
- Add `anthropic` crate (or call the API directly via `reqwest` — no official
  Rust SDK exists yet, so raw `reqwest` is fine; `reqwest` + `futures-util` are
  already present).

### `src/auth/mod.rs`
- Remove `mod github_oauth;`
- Remove `pub use enroll::{run_enroll, run_github_auth};` → keep only `run_enroll`
- Remove `pub use github_oauth::{begin_github_auth, complete_github_auth};`

### `src/auth/enroll.rs`
- Remove `run_github_auth()` function (lines ~131–200)
- Remove the `begin_github_auth` / `complete_github_auth` imports
- Remove the `GITHUB_REDIRECT_URI` constant if it exists only for GitHub auth

### `src/main.rs`
- Remove `GithubAuth` variant from `Command` enum
- Remove `Command::GithubAuth => auth::run_github_auth().await` match arm
- Remove the `/// Authenticate with GitHub…` doc comment

### `src/mcp/tools/analyse_colony.rs`
- Remove the `github_token` load from the token store
- Remove the `CopilotClient` import
- Instantiate a new `ClaudeClient` (see below) directly, reading
  `ANTHROPIC_API_KEY` from the environment at call time

### `src/mcp/tools/suggest_schedule.rs` (if it similarly uses CopilotClient)
- Same substitution as `analyse_colony.rs`

---

## Files to create

### `src/claude/mod.rs`
Module doc mirroring `src/copilot/mod.rs` style:
```
pub use client::ClaudeClient;
```

### `src/claude/client.rs`
Drop-in replacement for `src/copilot/client.rs`:

- Endpoint: `https://api.anthropic.com/v1/messages`
- Auth: `x-api-key: $ANTHROPIC_API_KEY` header
- Model: `claude-sonnet-4-6`
- Streaming: SSE (`"stream": true`) — parse `event: content_block_delta` /
  `data: {"delta":{"text":"…"}}` chunks
- Public API mirrors the old client exactly:
  - `ClaudeClient::new(api_key: &str) -> Self`
  - `async fn analyse_colony(&self, colony_json: &str) -> anyhow::Result<String>`
  - `async fn suggest_schedule(&self, schedule_json: &str) -> anyhow::Result<String>`
- Prompts: reuse the existing `prompts/system.md`, `prompts/analyse_colony.md`,
  `prompts/suggest_schedule.md` unchanged via `include_str!`

Move the prompts directory:
- `src/copilot/prompts/` → `src/claude/prompts/`

---

## Wiring

In `src/main.rs`:
```rust
mod claude; // replaces mod copilot
```

In `analyse_colony.rs` / `suggest_schedule.rs`:
```rust
use crate::claude::ClaudeClient;
// ...
let api_key = std::env::var("ANTHROPIC_API_KEY")
    .map_err(|_| anyhow::anyhow!("ANTHROPIC_API_KEY not set"))?;
let client = ClaudeClient::new(&api_key);
```

---

## What does NOT change

- `src/auth/store.rs` — token store is still used for EVE SSO tokens
- `src/copilot/prompts/*.md` — moved, not edited
- All MCP tool logic beyond the client instantiation
- `src/auth/enroll.rs` EVE SSO flow (`run_enroll`)
- The `github-auth` smoke test in the test suite (delete or skip it)

---

## Environment variable

| Old | New |
|-----|-----|
| `GITHUB_CLIENT_ID` + `GITHUB_CLIENT_SECRET` | `ANTHROPIC_API_KEY` |

Update `.env.example` / README if they document the old vars.

---

## Order of operations

1. Create `src/claude/client.rs` + `src/claude/mod.rs`, move prompts
2. Update `src/main.rs` — remove `GithubAuth`, add `mod claude`
3. Update `src/auth/mod.rs` + `src/auth/enroll.rs` — remove GitHub OAuth
4. Delete `src/auth/github_oauth.rs` + `src/copilot/`
5. Update `analyse_colony.rs` + `suggest_schedule.rs` to use `ClaudeClient`
6. `cargo build` — fix any remaining compile errors
7. Smoke test: set `ANTHROPIC_API_KEY` and call `analyse_colony` via MCP
