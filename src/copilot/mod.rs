//! GitHub Copilot API client.
//!
//! All calls to the Copilot API are routed through this module.
//! Never scatter raw HTTP calls to the Copilot API elsewhere.
//!
//! - Auth: Bearer token from GitHub OAuth (scope: `copilot`)
//! - Endpoint: https://api.githubcopilot.com/chat/completions
//! - Streaming: SSE (`stream: true`) for user-facing responses
//! - Explicit only: never called proactively from background tasks

mod client;

pub use client::CopilotClient;
