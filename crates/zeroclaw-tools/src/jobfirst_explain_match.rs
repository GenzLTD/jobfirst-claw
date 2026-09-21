//! JobFirst tool: explain match reasons for a job position

use async_trait::async_trait;
use serde_json::json;
use std::time::Duration;
use zeroclaw_api::tool::{Tool, ToolResult};

zeroclaw_api::tool_attribution!(JobFirstExplainMatchTool, zeroclaw_api::attribution::ToolKind::Plugin);

fn api_base_url() -> String {
    if is_tatha_backend() {
        std::env::var("JOBFIRST_API_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:5210".to_string())
    } else if is_smartjobs_backend() {
        std::env::var("SMARTJOBS_API_URL")
            .or_else(|_| std::env::var("JOBFIRST_API_URL"))
            .unwrap_or_else(|_| "http://47.115.168.107:9001/api/v1".to_string())
    } else {
        std::env::var("JOBFIRST_API_URL")
            .unwrap_or_else(|_| "http://localhost:3333".to_string())
    }
}

fn is_tatha_backend() -> bool {
    std::env::var("JOBFIRST_BACKEND").map(|v| v.to_lowercase() == "tatha").unwrap_or(false)
}

fn is_smartjobs_backend() -> bool {
    std::env::var("JOBFIRST_BACKEND").map(|v| v.to_lowercase() == "smartjobs").unwrap_or(false)
        || std::env::var("SMARTJOBS_API_URL").is_ok()
}

fn auth_token() -> Option<String> {
    std::env::var("ZERVIGO_TOKEN").or_else(|_| std::env::var("JOBFIRST_API_KEY")).ok()
}

pub struct JobFirstExplainMatchTool;

impl Default for JobFirstExplainMatchTool {
    fn default() -> Self { Self }
}

#[async_trait]
impl Tool for JobFirstExplainMatchTool {
    fn name(&self) -> &str { "jobfirst_explain_match" }

    fn description(&self) -> &str {
        "Explain match reasons for a job position. Tatha: provide job_index and resume_text/file_path. SmartJobs: provide job_id and resume_id."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "job_index": {"type": "integer", "description": "Match index (1-based, Tatha backend)"},
                "job_id": {"type": "string", "description": "Job ID (SmartJobs backend)"},
                "resume_text": {"type": "string", "description": "Resume text (Tatha backend)"},
                "file_path": {"type": "string", "description": "Local resume file path (Tatha)"},
                "resume_id": {"type": "string", "description": "Resume ID (SmartJobs backend)"}
            },
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let job_id = args.get("job_id").and_then(|v| v.as_str()).map(|s| s.trim().to_string());
        let job_index = args.get("job_index").and_then(|v| v.as_i64()).unwrap_or(1).max(1) as usize;
        let resume_text = args.get("resume_text").and_then(|v| v.as_str()).map(|s| s.trim().to_string());
        let file_path = args.get("file_path").and_then(|v| v.as_str()).map(|s| s.trim().to_string());
        let resume_id = args.get("resume_id").and_then(|v| v.as_str());

        if is_tatha_backend() {
            let mut resume_text_val = resume_text;
            if let Some(ref fp) = file_path {
                let path = std::path::Path::new(fp);
                if path.exists() {
                    match tokio::fs::read_to_string(path).await {
                        Ok(content) => resume_text_val = Some(content),
                        Err(e) => return Ok(ToolResult { success: false, output: String::new().into(), error: Some(format!("read failed: {}", e)) })
                    }
                } else {
                    return Ok(ToolResult { success: false, output: String::new().into(), error: Some(format!("file not found: {}", fp)) });
                }
            }
            let resume_text_val = match resume_text_val {
                Some(t) if !t.trim().is_empty() => t,
                _ => return Ok(ToolResult { success: false, output: String::new().into(), error: Some("Tatha backend needs resume_text or file_path".to_string()) })
            };

            let base = api_base_url().trim_end_matches('/').to_string();
            let url = format!("{}/v1/jobs/match", base);
            let body = json!({ "resume_text": resume_text_val, "top_n": 20 });
            let mut req = reqwest::Client::builder().timeout(Duration::from_secs(120)).build()?.post(&url).json(&body);
            if let Some(token) = auth_token() { req = req.header("Authorization", format!("Bearer {}", token)); }

            match req.send().await {
                Ok(resp) => {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    if !status.is_success() { return Ok(ToolResult { success: false, output: body.into(), error: Some(format!("Tatha API returned {}", status)) }); }
                    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap_or(json!({}));
                    let empty: Vec<serde_json::Value> = vec![];
                    let matches = parsed.get("matches").and_then(|m| m.as_array()).unwrap_or(&empty);
                    let empty_map = serde_json::Map::new();
                    let idx = job_index.saturating_sub(1);
                    let Some(m) = matches.get(idx) else {
                        return Ok(ToolResult { success: true, output: format!("Match #{} does not exist. {} results total.", job_index, matches.len()).into(), error: None });
                    };
                    let score = m.get("score").and_then(|s| s.as_object()).unwrap_or(&empty_map);
                    let job = m.get("job").and_then(|j| j.as_object()).unwrap_or(&empty_map);
                    let title = job.get("title").and_then(|v| v.as_str()).unwrap_or("-");
                    let company = job.get("company").and_then(|v| v.as_str()).unwrap_or("-");
                    let overall = score.get("overall").and_then(|v| v.as_i64()).unwrap_or(0);
                    let summary = score.get("summary").and_then(|v| v.as_str()).unwrap_or("");
                    let fit_bullets = score.get("fit_bullets").and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(String::from).collect::<Vec<_>>()).unwrap_or_default();
                    let mut lines = vec![format!("[{} | {}] Match: {}%", title, company, overall), String::new(), summary.to_string()];
                    if !fit_bullets.is_empty() {
                        lines.push(String::new());
                        lines.push("Fit highlights:".to_string());
                        for b in &fit_bullets { lines.push(format!("- {}", b)); }
                    }
                    Ok(ToolResult { success: true, output: lines.join("\n").into(), error: None })
                }
                Err(e) => Ok(ToolResult { success: false, output: String::new().into(), error: Some(format!("Tatha request failed: {}", e)) })
            }
        } else {
            let job_id = match job_id {
                Some(id) if !id.is_empty() => id,
                _ => return Ok(ToolResult { success: false, output: String::new().into(), error: Some("missing job_id parameter".to_string()) })
            };
            if is_smartjobs_backend() {
                let rid = resume_id.unwrap_or("");
                if rid.is_empty() {
                    return Ok(ToolResult { success: true, output: "SmartJobs: use POST /api/v1/resumes/improve with resume_id + job_id.".to_string().into(), error: None });
                }
                let base = api_base_url().trim_end_matches('/').to_string();
                let url = format!("{}/resumes/improve", base);
                let body = json!({ "resume_id": rid, "job_id": job_id });
                let mut req = reqwest::Client::builder().timeout(Duration::from_secs(120)).build()?.post(&url).json(&body);
                if let Some(token) = auth_token() { req = req.header("Authorization", format!("Bearer {}", token)); }
                return match req.send().await {
                    Ok(resp) => {
                        let status = resp.status();
                        let txt = resp.text().await.unwrap_or_default();
                        if !status.is_success() {
                            Ok(ToolResult { success: false, output: txt.into(), error: Some(format!("API returned {}", status)) })
                        } else {
                            Ok(ToolResult { success: true, output: txt.into(), error: None })
                        }
                    }
                    Err(e) => Ok(ToolResult { success: false, output: String::new().into(), error: Some(format!("request failed: {}", e)) })
                };
            }
            let base = api_base_url().trim_end_matches('/').to_string();
            let url = format!("{}/api/v1/job/{}/explain", base, job_id);
            let mut req = reqwest::Client::builder().timeout(Duration::from_secs(30)).build()?.get(&url);
            if let Some(token) = auth_token() { req = req.header("Authorization", format!("Bearer {}", token)); }
            match req.send().await {
                Ok(resp) => {
                    let body = resp.text().await.unwrap_or_default();
                    Ok(ToolResult { success: true, output: body.into(), error: None })
                }
                Err(e) => Ok(ToolResult { success: false, output: String::new().into(), error: Some(format!("request failed: {}", e)) })
            }
        }
    }
}
