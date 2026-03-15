use serde_json::{Value, json};

use super::EvepiService;
use crate::copilot::CopilotClient;

pub async fn handle(
    svc: &EvepiService,
    character_id: i64,
    planet_id: i64,
) -> anyhow::Result<Value> {
    // Require GitHub token — fail clearly if not authenticated
    let github_token = svc
        .store
        .load("github_token")
        .map_err(|_| anyhow::anyhow!("GitHub token not found. Run the GitHub auth flow first."))?
        .access_token;

    // Build colony context for the prompt
    let layout = crate::mcp::tools::get_colony_layout::handle(svc, character_id, planet_id).await?;

    let colony_json = serde_json::to_string_pretty(&layout)?;

    let client = CopilotClient::new(&github_token);
    let analysis = client.analyse_colony(&colony_json).await?;

    Ok(json!({
        "character_id": character_id,
        "planet_id": planet_id,
        "analysis": analysis,
    }))
}
