//! Handler for the `start_eve_auth` MCP tool.
//!
//! Generates an EVE SSO authorization URL that the user can open in a browser.
//! The in-flight PKCE session is stored in the `pending_auth` DB table so the
//! background callback server can complete the exchange when the browser
//! redirects back to `http://localhost:7878/callback`.

use anyhow::Context as _;

use crate::auth::begin_eve_auth;
use crate::mcp::tools::EvepiService;

const REDIRECT_URI: &str = "http://localhost:7878/callback";

pub async fn handle(service: &EvepiService, account: Option<String>) -> anyhow::Result<String> {
    // Start the callback server if it isn't already running.
    service
        .callback_server
        .ensure_running(
            service.db.clone(),
            service.store.clone(),
            service.client_id.clone(),
        )
        .await
        .context("could not start EVE SSO callback server")?;

    // Generate a fresh PKCE session.
    let (url, session) = begin_eve_auth(&service.client_id, REDIRECT_URI)
        .context("failed to generate EVE SSO authorization URL")?;

    // Persist the in-flight session so the callback server can complete it.
    sqlx::query(
        "INSERT INTO pending_auth (state, pkce_verifier, redirect_uri, account_label)
         VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(&session.state)
    .bind(&session.pkce_verifier)
    .bind(REDIRECT_URI)
    .bind(&account)
    .execute(service.db.pool())
    .await
    .context("failed to store pending auth session")?;

    let account_hint = account
        .as_deref()
        .map(|a| format!(" (account: **{a}**)"))
        .unwrap_or_default();

    Ok(format!(
        "Open this URL in your browser to authorise a character{account_hint}:\n\n\
         {url}\n\n\
         Once the browser shows the success page the character is enrolled. \
         Run `sync_characters` to pull their colony data."
    ))
}
