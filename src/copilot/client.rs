use anyhow::Context as _;
use futures_util::StreamExt as _;
use reqwest::Client;
use serde_json::{Value, json};

const COPILOT_ENDPOINT: &str = "https://api.githubcopilot.com/chat/completions";
const COPILOT_MODEL: &str = "gpt-4o";

/// Client for the GitHub Copilot chat completions API.
pub struct CopilotClient {
    http: Client,
    token: String,
}

impl CopilotClient {
    pub fn new(github_token: &str) -> Self {
        Self {
            http: Client::new(),
            token: github_token.to_owned(),
        }
    }

    /// Analyse a single colony's efficiency. Returns the full response text.
    pub async fn analyse_colony(&self, colony_json: &str) -> anyhow::Result<String> {
        let system_prompt = include_str!("prompts/system.md");
        let user_prompt = format!(
            "{}\n\n## Colony Data\n```json\n{}\n```",
            include_str!("prompts/analyse_colony.md"),
            colony_json
        );
        self.chat(system_prompt, &user_prompt).await
    }

    /// Generate a ranked reset schedule recommendation.
    pub async fn suggest_schedule(&self, schedule_json: &str) -> anyhow::Result<String> {
        let system_prompt = include_str!("prompts/system.md");
        let user_prompt = format!(
            "{}\n\n## Schedule Data\n```json\n{}\n```",
            include_str!("prompts/suggest_schedule.md"),
            schedule_json
        );
        self.chat(system_prompt, &user_prompt).await
    }

    /// Send a chat completion request and collect the full streamed response.
    async fn chat(&self, system: &str, user: &str) -> anyhow::Result<String> {
        let body = json!({
            "model": COPILOT_MODEL,
            "stream": true,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user",   "content": user   },
            ],
        });

        let resp = self
            .http
            .post(COPILOT_ENDPOINT)
            .bearer_auth(&self.token)
            .header("Content-Type", "application/json")
            .header("Copilot-Integration-Id", "evepi-server")
            .json(&body)
            .send()
            .await
            .context("Copilot API request failed")?
            .error_for_status()
            .context("Copilot API returned error status")?;

        let mut stream = resp.bytes_stream();
        let mut output = String::new();

        while let Some(chunk) = stream.next().await {
            let bytes = chunk.context("error reading Copilot SSE stream")?;
            let text = std::str::from_utf8(&bytes).unwrap_or_default();

            for line in text.lines() {
                if let Some(data) = line.strip_prefix("data: ") {
                    if data == "[DONE]" {
                        break;
                    }
                    if let Ok(val) = serde_json::from_str::<Value>(data)
                        && let Some(content) = val
                            .pointer("/choices/0/delta/content")
                            .and_then(|v| v.as_str())
                    {
                        output.push_str(content);
                    }
                }
            }
        }

        Ok(output)
    }
}
