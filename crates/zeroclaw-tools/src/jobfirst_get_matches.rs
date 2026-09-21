//! JobFirst tool: get job position matches

use crate::jobfirst_resume_store;
use async_trait::async_trait;
use serde_json::json;
use std::time::Duration;
use zeroclaw_api::tool::{Tool, ToolResult};

zeroclaw_api::tool_attribution!(JobFirstGetMatchesTool, zeroclaw_api::attribution::ToolKind::Plugin);

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
    std::env::var("JOBFIRST_BACKEND")
        .map(|v| v.to_lowercase() == "tatha")
        .unwrap_or(false)
}

fn is_smartjobs_backend() -> bool {
    std::env::var("JOBFIRST_BACKEND")
        .map(|v| v.to_lowercase() == "smartjobs")
        .unwrap_or(false)
        || std::env::var("SMARTJOBS_API_URL").is_ok()
}

fn auth_token() -> Option<String> {
    std::env::var("ZERVIGO_TOKEN")
        .or_else(|_| std::env::var("JOBFIRST_API_KEY"))
        .ok()
}

async fn tatha_convert_file(path: &std::path::Path) -> anyhow::Result<String> {
    let bytes = tokio::fs::read(path).await?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("resume.pdf").to_string();
    let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).unwrap_or_default();
    let mime = match ext.as_str() {
        "pdf" => "application/pdf",
        "docx" | "doc" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xlsx" | "xls" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        _ => "application/octet-stream",
    };
    let part = reqwest::multipart::Part::bytes(bytes).file_name(name).mime_str(mime)
        .map_err(|e| anyhow::anyhow!("multipart: {}", e))?;
    let form = reqwest::multipart::Form::new().part("file", part).text("document_type", "resume");

    let base = api_base_url().trim_end_matches('/').to_string();
    let url = format!("{}/v1/documents/convert", base);
    let mut req = reqwest::Client::builder().timeout(Duration::from_secs(60)).build()?.post(&url).multipart(form);
    if let Some(token) = auth_token() { req = req.header("Authorization", format!("Bearer {}", token)); }

    let resp = req.send().await?;
    let status = resp.status();
    let body = resp.text().await?;
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap_or(json!({"raw": body}));
    if !status.is_success() { anyhow::bail!("Tatha convert returned {}: {}", status, body); }
    if let Some(err) = parsed.get("error").and_then(|v| v.as_str()) { anyhow::bail!("conversion failed: {}", err); }
    let markdown = parsed.get("markdown").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if markdown.trim().is_empty() { anyhow::bail!("empty conversion result"); }
    Ok(markdown)
}

pub struct JobFirstGetMatchesTool;

impl Default for JobFirstGetMatchesTool {
    fn default() -> Self { Self }
}

#[async_trait]
impl Tool for JobFirstGetMatchesTool {
    fn name(&self) -> &str { "jobfirst_get_matches" }

    fn description(&self) -> &str {
        "Get job position matches based on resume. Returns Top-N positions with match scores. Use file_path for PDF/DOCX/TXT files (not pdf_read). Tatha backend supports any path. After jobfirst_upload_resume, can call without params."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "resume_text": {"type": "string", "description": "Resume text or summary (required for Tatha)"},
                "file_path": {"type": "string", "description": "Local resume file path (PDF/DOCX/TXT)"},
                "resume_id": {"type": "string", "description": "Resume ID (SmartJobs backend)"},
                "user_id": {"type": "string", "description": "User ID, optional"},
                "top_n": {"type": "integer", "description": "Top N matches, default 5, max 20", "default": 5}
            },
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let resume_text = args.get("resume_text").and_then(|v| v.as_str()).map(|s| s.trim().to_string());
        let file_path = args.get("file_path").and_then(|v| v.as_str()).map(|s| s.trim().to_string());
        let resume_id = args.get("resume_id").and_then(|v| v.as_str()).map(String::from);
        let user_id = args.get("user_id").and_then(|v| v.as_str()).map(String::from);
        let top_n = args.get("top_n").and_then(|v| v.as_i64()).unwrap_or(5).min(20).max(1) as u32;

        if is_tatha_backend() {
            let mut resume_text_val = resume_text;
            if let Some(ref fp) = file_path {
                let path = std::path::Path::new(fp);
                if !path.exists() {
                    return Ok(ToolResult { success: false, output: String::new().into(), error: Some(format!("file not found: {}", fp)) });
                }
                let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).unwrap_or_default();
                let content = if ext == "pdf" || ext == "docx" || ext == "doc" || ext == "xlsx" || ext == "xls" {
                    match tatha_convert_file(path).await {
                        Ok(markdown) => { jobfirst_resume_store::set_resume(markdown.clone()); markdown }
                        Err(e) => return Ok(ToolResult { success: false, output: String::new().into(), error: Some(format!("parse failed: {}", e)) })
                    }
                } else {
                    match tokio::fs::read_to_string(path).await {
                        Ok(c) => c,
                        Err(e) => return Ok(ToolResult { success: false, output: String::new().into(), error: Some(format!("read failed: {}", e)) })
                    }
                };
                resume_text_val = Some(content);
            }

            if resume_text_val.as_ref().map(|s| s.trim().is_empty()).unwrap_or(true) {
                resume_text_val = jobfirst_resume_store::get_resume();
            }

            let resume_text_val = match resume_text_val {
                Some(t) if !t.trim().is_empty() => t,
                _ => return Ok(ToolResult { success: false, output: String::new().into(), error: Some("no resume found. Use jobfirst_upload_resume first, or provide resume_text/file_path.".to_string()) })
            };

            let base = api_base_url().trim_end_matches('/').to_string();
            let url = format!("{}/v1/jobs/match", base);
            let body = json!({ "resume_text": resume_text_val, "top_n": top_n });
            let mut req = reqwest::Client::builder().timeout(Duration::from_secs(120)).build()?.post(&url).json(&body);
            if let Some(token) = auth_token() { req = req.header("Authorization", format!("Bearer {}", token)); }

            match req.send().await {
                Ok(resp) => {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    if !status.is_success() { return Ok(ToolResult { success: false, output: body.into(), error: Some(format!("Tatha API returned {}", status)) }); }
                    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap_or(json!({"raw": body}));
                    let empty_map = serde_json::Map::new();
                    let payload = parsed.get("data").and_then(|d| d.as_object()).or_else(|| parsed.as_object()).unwrap_or(&empty_map);
                    let total = payload.get("total_evaluated").and_then(|v| v.as_i64()).unwrap_or(0);
                    let empty: Vec<serde_json::Value> = vec![];
                    let matches = payload.get("matches").and_then(|m| m.as_array()).unwrap_or(&empty);
                    let lines: Vec<String> = matches.iter().take(top_n as usize).enumerate().map(|(i, m)| {
                        let job = m.get("job").and_then(|j| j.as_object()).unwrap_or(&empty_map);
                        let score = m.get("score").and_then(|s| s.as_object()).unwrap_or(&empty_map);
                        let title = job.get("title").and_then(|v| v.as_str()).unwrap_or("-");
                        let company = job.get("company").and_then(|v| v.as_str()).unwrap_or("-");
                        let overall = score.get("overall").and_then(|v| v.as_i64()).unwrap_or(0);
                        format!("{}. {} | {} | match: {}%", i + 1, title, company, overall)
                    }).collect();
                    let output = if lines.is_empty() { format!("No matching positions (evaluated {} entries)", total) }
                                 else { format!("Evaluated {} entries, Top-{} matches:\n\n{}", total, lines.len(), lines.join("\n\n")) };
                    Ok(ToolResult { success: true, output: output.into(), error: None })
                }
                Err(e) => Ok(ToolResult { success: false, output: String::new().into(), error: Some(format!("Tatha request failed: {}. Check Tatha (8010) and JOBFIRST_API_URL={}", e, api_base_url())) })
            }
        } else {
            if resume_id.is_none() && user_id.is_none() {
                return Ok(ToolResult { success: false, output: String::new().into(), error: Some("need resume_id or user_id".to_string()) });
            }
            if is_smartjobs_backend() {
                return Ok(ToolResult { success: true, output: "SmartJobs backend: use POST /api/v1/resumes/improve with resume_id + job_id.".to_string().into(), error: None });
            }
            let base = api_base_url().trim_end_matches('/').to_string();
            let url = if let Some(rid) = &resume_id { format!("{}/api/v1/job/matches?resume_id={}&top_n={}", base, rid, top_n) }
                      else { format!("{}/api/v1/job/matches?user_id={}&top_n={}", base, user_id.unwrap_or_default(), top_n) };
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
