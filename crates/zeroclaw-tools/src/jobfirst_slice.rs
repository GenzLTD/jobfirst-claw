//! JobFirst M0 tools — Slice API (inbox narrative)

use crate::jobfirst_resume_store;
use async_trait::async_trait;
use serde_json::{Value, json};
use std::time::Duration;
use zeroclaw_api::tool::{Tool, ToolResult};

/// Slice API 基地址。
///
/// 默认走 **api-gateway**（`:3333` 的 `/api/v1/slice/*`），而不是直连
/// `jobfirst-slice-service`（`:3005`）：网关是生态的唯一入口，直连会绕过
/// 网关侧鉴权与身份注入（`X-User-Id = sha256(email)`）。
///
/// 仅在本机联调、且明确不需要网关时，才用
/// `JOBFIRST_SLICE_URL=http://127.0.0.1:3005` 覆盖回直连。
fn slice_base_url() -> String {
    std::env::var("JOBFIRST_SLICE_URL")
        .unwrap_or_else(|_| "http://127.0.0.1:3333/api/v1/slice".to_string())
        .trim_end_matches('/')
        .to_string()
}

fn http_client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?)
}

fn slice_data(body: &Value) -> anyhow::Result<&Value> {
    let code = body.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
    if code != 0 {
        let msg = body
            .get("msg")
            .and_then(|v| v.as_str())
            .unwrap_or("slice API error");
        anyhow::bail!("{}", msg);
    }
    body.get("data")
        .ok_or_else(|| anyhow::Error::msg("slice response missing data"))
}

async fn slice_post(path: &str, payload: Value) -> anyhow::Result<Value> {
    let url = format!("{}/{}", slice_base_url(), path.trim_start_matches('/'));
    let resp = http_client()?.post(&url).json(&payload).send().await?;
    let status = resp.status();
    let body: Value = resp.json().await?;
    if !status.is_success() {
        anyhow::bail!("HTTP {}: {}", status, body);
    }
    Ok(body)
}

async fn slice_get(path: &str, query: &str) -> anyhow::Result<Value> {
    let url = if query.is_empty() {
        format!("{}/{}", slice_base_url(), path.trim_start_matches('/'))
    } else {
        format!(
            "{}/{}?{}",
            slice_base_url(),
            path.trim_start_matches('/'),
            query
        )
    };
    let resp = http_client()?.get(&url).send().await?;
    let status = resp.status();
    let body: Value = resp.json().await?;
    if !status.is_success() {
        anyhow::bail!("HTTP {}: {}", status, body);
    }
    Ok(body)
}

fn resolve_resume_id(args: &Value) -> anyhow::Result<String> {
    if let Some(id) = args.get("resume_id").and_then(|v| v.as_str()) {
        let id = id.trim();
        if !id.is_empty() {
            return Ok(id.to_string());
        }
    }
    jobfirst_resume_store::get_resume_id()
        .ok_or_else(|| anyhow::Error::msg("no resume_id; call jobfirst_slice_submit_resume first"))
}

// --- submit resume ---

pub struct JobFirstSliceSubmitResumeTool;

impl Default for JobFirstSliceSubmitResumeTool {
    fn default() -> Self {
        Self
    }
}

impl JobFirstSliceSubmitResumeTool {
    pub fn new() -> Self {
        Self
    }
}

zeroclaw_api::tool_attribution!(
    JobFirstSliceSubmitResumeTool,
    zeroclaw_api::attribution::ToolKind::Plugin
);

#[async_trait]
impl Tool for JobFirstSliceSubmitResumeTool {
    fn name(&self) -> &str {
        "jobfirst_slice_submit_resume"
    }

    fn description(&self) -> &str {
        "Submit resume text to JobFirst Slice API. Stores resume_id for later apply/inbox tools. M0 inbox path — not legacy /job API."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "resume_text": { "type": "string", "description": "Resume plain text" },
                "file_path": { "type": "string", "description": "Optional local .txt resume file" }
            },
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        let mut text = args
            .get("resume_text")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if let Some(fp) = args.get("file_path").and_then(|v| v.as_str()) {
            let path = std::path::Path::new(fp.trim());
            if path.exists() {
                text = tokio::fs::read_to_string(path).await?;
            } else {
                return Ok(ToolResult {
                    success: false,
                    output: String::new().into(),
                    error: Some(format!("file not found: {}", fp)),
                });
            }
        }
        if text.trim().is_empty() {
            return Ok(ToolResult {
                success: false,
                output: String::new().into(),
                error: Some("resume_text or file_path required".into()),
            });
        }

        let body = slice_post("resume", json!({ "text": text.trim() })).await?;
        let data = slice_data(&body)?;
        let resume_id = data
            .get("resume_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow::Error::msg("missing resume_id in response"))?;
        jobfirst_resume_store::set_resume(text);
        jobfirst_resume_store::set_resume_id(resume_id.to_string());

        Ok(ToolResult {
            success: true,
            output: serde_json::to_string_pretty(&data)
                .map_err(|e| anyhow::Error::msg(format!("json: {}", e)))?
                .into(),
            error: None,
        })
    }
}

// --- apply batch ---

pub struct JobFirstSliceApplyTool;

impl Default for JobFirstSliceApplyTool {
    fn default() -> Self {
        Self
    }
}

impl JobFirstSliceApplyTool {
    pub fn new() -> Self {
        Self
    }
}

zeroclaw_api::tool_attribution!(
    JobFirstSliceApplyTool,
    zeroclaw_api::attribution::ToolKind::Plugin
);

#[async_trait]
impl Tool for JobFirstSliceApplyTool {
    fn name(&self) -> &str {
        "jobfirst_slice_run_apply"
    }

    fn description(&self) -> &str {
        "Agent batch apply: fetch Top-N matches from Slice then POST /apply. Uses stored resume_id if omitted. M0 simulated apply — not real external job boards."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "resume_id": { "type": "string" },
                "top": { "type": "integer", "description": "When job_ids omitted, apply top N matches (default 3)" },
                "job_ids": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Explicit job ids to apply"
                }
            },
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        let resume_id = resolve_resume_id(&args)?;
        let top = args.get("top").and_then(|v| v.as_u64()).unwrap_or(3).max(1) as u32;

        let job_ids: Vec<String> = if let Some(arr) = args.get("job_ids").and_then(|v| v.as_array())
        {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        } else {
            let matches_body =
                slice_get("matches", &format!("resume_id={}&top={}", resume_id, top)).await?;
            let data = slice_data(&matches_body)?;
            data.get("matches")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|m| {
                            m.get("job_id")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string())
                        })
                        .collect()
                })
                .unwrap_or_default()
        };

        if job_ids.is_empty() {
            return Ok(ToolResult {
                success: false,
                output: String::new().into(),
                error: Some("no job_ids to apply".into()),
            });
        }

        let body = slice_post(
            "apply",
            json!({ "resume_id": resume_id, "job_ids": job_ids }),
        )
        .await?;
        let data = slice_data(&body)?;

        Ok(ToolResult {
            success: true,
            output: serde_json::to_string_pretty(&data)
                .map_err(|e| anyhow::Error::msg(format!("json: {}", e)))?
                .into(),
            error: None,
        })
    }
}

// --- list inbox ---

pub struct JobFirstSliceListInboxTool;

impl Default for JobFirstSliceListInboxTool {
    fn default() -> Self {
        Self
    }
}

impl JobFirstSliceListInboxTool {
    pub fn new() -> Self {
        Self
    }
}

zeroclaw_api::tool_attribution!(
    JobFirstSliceListInboxTool,
    zeroclaw_api::attribution::ToolKind::Plugin
);

#[async_trait]
impl Tool for JobFirstSliceListInboxTool {
    fn name(&self) -> &str {
        "jobfirst_slice_list_inbox"
    }

    fn description(&self) -> &str {
        "List feedback inbox for a resume_id (applications and statuses). User-facing M0 narrative."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "resume_id": { "type": "string" }
            },
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        let resume_id = resolve_resume_id(&args)?;
        let body = slice_get("inbox", &format!("resume_id={}", resume_id)).await?;
        let data = slice_data(&body)?;
        Ok(ToolResult {
            success: true,
            output: serde_json::to_string_pretty(&data)
                .map_err(|e| anyhow::Error::msg(format!("json: {}", e)))?
                .into(),
            error: None,
        })
    }
}

// --- inbox decision ---

pub struct JobFirstSliceInboxDecisionTool;

impl Default for JobFirstSliceInboxDecisionTool {
    fn default() -> Self {
        Self
    }
}

impl JobFirstSliceInboxDecisionTool {
    pub fn new() -> Self {
        Self
    }
}

zeroclaw_api::tool_attribution!(
    JobFirstSliceInboxDecisionTool,
    zeroclaw_api::attribution::ToolKind::Plugin
);

#[async_trait]
impl Tool for JobFirstSliceInboxDecisionTool {
    fn name(&self) -> &str {
        "jobfirst_slice_inbox_decision"
    }

    fn description(&self) -> &str {
        "User decision on feedback: interview or skip. action must be \"interview\" or \"skip\"."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "application_id": { "type": "string" },
                "action": {
                    "type": "string",
                    "enum": ["interview", "skip"],
                    "description": "interview = worth scheduling; skip = pass"
                }
            },
            "required": ["application_id", "action"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: Value) -> anyhow::Result<ToolResult> {
        let app_id = args
            .get("application_id")
            .and_then(|v| v.as_str())
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::Error::msg("application_id required"))?;
        let action = args
            .get("action")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_lowercase())
            .unwrap_or_default();
        if action != "interview" && action != "skip" {
            return Ok(ToolResult {
                success: false,
                output: String::new().into(),
                error: Some("action must be interview or skip".into()),
            });
        }

        let url_path = format!("inbox/{}/decision", app_id);
        let body = slice_post(&url_path, json!({ "action": action })).await?;
        let data = slice_data(&body)?;

        Ok(ToolResult {
            success: true,
            output: serde_json::to_string_pretty(&data)
                .map_err(|e| anyhow::Error::msg(format!("json: {}", e)))?
                .into(),
            error: None,
        })
    }
}
