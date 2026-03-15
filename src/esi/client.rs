use anyhow::Context as _;
use reqwest::{Client, Response, StatusCode};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use super::models::{
    EsiCharacterLocation, EsiColony, EsiColonyLayout, EsiCustomsOffice, EsiSchematic,
    EsiSolarSystem, EsiType,
};

const ESI_BASE: &str = "https://esi.evetech.net/latest";
/// Back off when fewer than this many ESI error tokens remain.
const ESI_ERROR_LIMIT_BACKOFF: i64 = 20;

/// Thread-safe ESI client. Reuse a single instance across the app.
#[derive(Debug, Clone)]
pub struct EsiClient {
    http: Client,
    /// Tracks remaining error budget from the last ESI response header.
    error_limit_remain: Arc<AtomicI64>,
}

impl EsiClient {
    pub fn new() -> anyhow::Result<Self> {
        let http = Client::builder()
            .user_agent(concat!(
                "evepi-server/",
                env!("CARGO_PKG_VERSION"),
                " (https://github.com/merlbrian/EvePI)"
            ))
            .build()
            .context("failed to build reqwest Client")?;
        Ok(Self {
            http,
            error_limit_remain: Arc::new(AtomicI64::new(100)),
        })
    }

    pub fn http_client(&self) -> &Client {
        &self.http
    }

    // -----------------------------------------------------------------------
    // PI endpoints
    // -----------------------------------------------------------------------

    /// `GET /characters/{character_id}/planets/`
    pub async fn list_colonies(
        &self,
        character_id: i64,
        token: &str,
    ) -> anyhow::Result<Vec<EsiColony>> {
        let url = format!("{ESI_BASE}/characters/{character_id}/planets/");
        let resp = self.get_authed(&url, token).await?;
        resp.json().await.context("list_colonies parse failed")
    }

    /// `GET /characters/{character_id}/planets/{planet_id}/`
    pub async fn get_colony_layout(
        &self,
        character_id: i64,
        planet_id: i64,
        token: &str,
    ) -> anyhow::Result<EsiColonyLayout> {
        let url = format!("{ESI_BASE}/characters/{character_id}/planets/{planet_id}/");
        let resp = self.get_authed(&url, token).await?;
        resp.json().await.context("get_colony_layout parse failed")
    }

    /// `GET /characters/{character_id}/location/`
    pub async fn get_character_location(
        &self,
        character_id: i64,
        token: &str,
    ) -> anyhow::Result<EsiCharacterLocation> {
        let url = format!("{ESI_BASE}/characters/{character_id}/location/");
        let resp = self.get_authed(&url, token).await?;
        resp.json()
            .await
            .context("get_character_location parse failed")
    }

    /// `GET /universe/systems/{system_id}/`
    pub async fn get_solar_system(&self, system_id: i64) -> anyhow::Result<EsiSolarSystem> {
        let url = format!("{ESI_BASE}/universe/systems/{system_id}/");
        let resp = self.get(&url).await?;
        resp.json().await.context("get_solar_system parse failed")
    }

    /// `GET /universe/schematics/{schematic_id}/`
    pub async fn get_schematic(&self, schematic_id: u32) -> anyhow::Result<EsiSchematic> {
        let url = format!("{ESI_BASE}/universe/schematics/{schematic_id}/");
        let resp = self.get(&url).await?;
        resp.json().await.context("get_schematic parse failed")
    }

    /// `GET /universe/types/{type_id}/`
    ///
    /// Returns only `type_id` and `name`; the full ESI response contains many
    /// more fields that are silently ignored by `EsiType`.
    pub async fn get_type_name(&self, type_id: i64) -> anyhow::Result<EsiType> {
        let url = format!("{ESI_BASE}/universe/types/{type_id}/");
        let resp = self.get(&url).await?;
        resp.json().await.context("get_type_name parse failed")
    }

    /// `GET /corporations/{corporation_id}/customs_offices/`
    pub async fn get_customs_offices(
        &self,
        corporation_id: i64,
        token: &str,
    ) -> anyhow::Result<Vec<EsiCustomsOffice>> {
        let url = format!("{ESI_BASE}/corporations/{corporation_id}/customs_offices/");
        let resp = self.get_authed(&url, token).await?;
        resp.json()
            .await
            .context("get_customs_offices parse failed")
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    async fn get(&self, url: &str) -> anyhow::Result<Response> {
        self.send(self.http.get(url)).await
    }

    async fn get_authed(&self, url: &str, token: &str) -> anyhow::Result<Response> {
        self.send(
            self.http
                .get(url)
                .header("Authorization", format!("Bearer {token}")),
        )
        .await
    }

    async fn send(&self, req: reqwest::RequestBuilder) -> anyhow::Result<Response> {
        // Check error budget before sending.
        if self.error_limit_remain.load(Ordering::Relaxed) < ESI_ERROR_LIMIT_BACKOFF {
            anyhow::bail!(
                "ESI error budget low ({}), backing off",
                self.error_limit_remain.load(Ordering::Relaxed)
            );
        }

        let resp = req.send().await.context("ESI request failed")?;

        // Update error budget from response headers.
        if let Some(val) = resp
            .headers()
            .get("X-ESI-Error-Limit-Remain")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<i64>().ok())
        {
            self.error_limit_remain.store(val, Ordering::Relaxed);
        }

        if resp.status() == StatusCode::TOO_MANY_REQUESTS
            || resp.status() == StatusCode::SERVICE_UNAVAILABLE
        {
            anyhow::bail!("ESI rate-limit hit: {}", resp.status());
        }

        resp.error_for_status()
            .context("ESI returned an error status")
    }
}
