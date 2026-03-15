//! ESI HTTP client module.
//!
//! Owns a single `reqwest::Client`, applies rate-limit backoff based on
//! `X-ESI-Error-Limit-Remain` / `X-ESI-Error-Limit-Reset` headers, and
//! exposes typed methods for every PI endpoint used by the app.

mod client;
pub mod models;

pub use client::EsiClient;
