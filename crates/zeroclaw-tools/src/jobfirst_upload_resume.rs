//! JobFirst tool: upload and parse resume
//! Tatha backend: POST /v1/documents/convert → store in-session

use crate::jobfirst_resume_store;
use async_trait::async_trait;
use serde_json::json;
use std::time::Duration;
use zeroclaw_api::tool::{Tool, ToolResult};

zeroclaw_api::tool_attribution!(
    JobFirstUploadResumeTool,
    zeroclaw_api::attribution::ToolKind::Plugin
);

fn is_tatha_backend() -> bool {
    std::env::var("JOBFIRST_BACKEND")
        .map(|v| v.to_lowercase() == "tatha")
        .unwrap_or(false)
}

fn api_base_url() -> String {
    if is_tatha_backend() {
        std::env::var("JOBFIRST_API_URL").unwrap_or_else(|_| "http://127.0.0.1:5210".to_string())
    } else if is_smartjobs_backend() {
        std::env::var("SMARTJOBS_API_URL")
            .or_else(|_| std::env::var("JOBFIRST_API_URL"))
            .unwrap_or_else(|_| "http://47.115.168.107:9001/api/v1".to_string())
    } else {
        std::env::var("JOBFIRST_API_URL").unwrap_or_else(|_| "http://localhost:3333".to_string())
    }
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

#[cfg(windows)]
fn normalize_file_path(s: &str) -> String {
    let s = s.trim().replace('/', "\\");
    let s = s.trim_start_matches('"').trim_end_matches('"');
    if s.starts_with(r"\\Users") || s.starts_with(r"\Users") {
        return format!("C:\\{}", s.trim_start_matches('\\'));
    }
    if s.starts_with("/Users/") {
        return s.replacen("/Users/", "C:\\Users\\", 1);
    }
    s.to_string()
}

#[cfg(not(windows))]
fn normalize_file_path(s: &str) -> String {
    s.trim()
        .trim_start_matches('"')
        .trim_end_matches('"')
        .to_string()
}

#[derive(Default)]
pub struct JobFirstUploadResumeTool {
    _placeholder: (),
}

impl JobFirstUploadResumeTool {
    pub fn new() -> Self {
        Self { _placeholder: () }
    }
}

#[async_trait]
impl Tool for JobFirstUploadResumeTool {
    fn name(&self) -> &str {
        "jobfirst_upload_resume"
    }

    fn description(&self) -> &str {
        "Upload resume file to JobFirst for parsing. Supports PDF, DOCX, TXT. Requires file_path parameter (local absolute path). Tatha backend stores in-session for subsequent matching."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "Local resume file absolute path"
                }
            },
            "required": ["file_path"],
            "additionalProperties": false
        })
    }

    async fn execute(&self, args: serde_json::Value) -> anyhow::Result<ToolResult> {
        let raw_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        let file_path = normalize_file_path(raw_path);

        if file_path.is_empty() {
            return Ok(ToolResult {
                success: false,
                output: String::new().into(),
                error: Some("missing file_path parameter".to_string()),
            });
        }

        let path = std::path::Path::new(&file_path);
        if !path.exists() {
            return Ok(ToolResult {
                success: false,
                output: String::new().into(),
                error: Some(format!("file not found: {}", file_path)),
            });
        }

        let bytes = tokio::fs::read(path)
            .await
            .map_err(|e| anyhow::Error::msg(format!("read failed: {}", e)))?;
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("resume.pdf")
            .to_string();

        if is_tatha_backend() {
            let base_url = api_base_url();
            let base = base_url.trim_end_matches('/');
            let url = format!("{}/v1/documents/convert", base);
            let mime = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| match e.to_lowercase().as_str() {
                    "pdf" => "application/pdf",
                    "docx" | "doc" => {
                        "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                    }
                    "txt" => "text/plain",
                    _ => "application/octet-stream",
                })
                .unwrap_or("application/octet-stream");

            let part = reqwest::multipart::Part::bytes(bytes)
                .file_name(file_name.clone())
                .mime_str(mime)
                .map_err(|e| anyhow::Error::msg(format!("multipart: {}", e)))?;
            let form = reqwest::multipart::Form::new()
                .part("file", part)
                .text("document_type", "resume");

            let mut req = reqwest::Client::builder()
                .timeout(Duration::from_secs(60))
                .build()?
                .post(&url)
                .multipart(form);
            if let Some(token) = auth_token() {
                req = req.header("Authorization", format!("Bearer {}", token));
            }

            match req.send().await {
                Ok(resp) => {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    if !status.is_success() {
                        return Ok(ToolResult {
                            success: false,
                            output: body.into(),
                            error: Some(format!("Tatha convert API returned {}", status)),
                        });
                    }
                    let parsed: serde_json::Value =
                        serde_json::from_str(&body).unwrap_or(json!({"raw": body}));
                    let markdown = parsed
                        .get("markdown")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if markdown.trim().is_empty() {
                        return Ok(ToolResult {
                            success: false,
                            output: body.into(),
                            error: Some("empty conversion result".to_string()),
                        });
                    }
                    jobfirst_resume_store::set_resume(markdown.clone());
                    Ok(ToolResult { success: true, output: format!("Resume uploaded and parsed. Use 'help me match jobs' to get recommendations.\nPreview:\n{}", if markdown.len() > 200 { format!("{}...", &markdown[..200]) } else { markdown }).into(), error: None })
                }
                Err(e) => Ok(ToolResult {
                    success: false,
                    output: String::new().into(),
                    error: Some(format!(
                        "Tatha request failed: {}. Check JOBFIRST_API_URL({}).",
                        e,
                        api_base_url()
                    )),
                }),
            }
        } else {
            let base_url = api_base_url();
            let base = base_url.trim_end_matches('/');
            let url = if is_smartjobs_backend() {
                format!("{}/resumes/upload", base)
            } else {
                format!("{}/api/v1/upload/upload", base)
            };
            let mime = path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| match e.to_lowercase().as_str() {
                    "pdf" => "application/pdf",
                    "docx" | "doc" => {
                        "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                    }
                    _ => "application/octet-stream",
                })
                .unwrap_or("application/octet-stream");

            let part = reqwest::multipart::Part::bytes(bytes)
                .file_name(file_name.clone())
                .mime_str(mime)
                .map_err(|e| anyhow::Error::msg(format!("multipart: {}", e)))?;
            let form = reqwest::multipart::Form::new().part("file", part);

            let mut req = reqwest::Client::builder()
                .timeout(Duration::from_secs(60))
                .build()?
                .post(&url)
                .multipart(form);
            if let Some(token) = auth_token() {
                req = req.header("Authorization", format!("Bearer {}", token));
            }

            match req.send().await {
                Ok(resp) => {
                    let status = resp.status();
                    let body = resp.text().await.unwrap_or_default();
                    if !status.is_success() {
                        let err_msg = format!("API returned {}: {}", status, body);
                        return Ok(ToolResult {
                            success: false,
                            output: body.into(),
                            error: Some(err_msg),
                        });
                    }
                    let parsed: serde_json::Value =
                        serde_json::from_str(&body).unwrap_or(json!({"raw": body}));
                    let summary = if is_smartjobs_backend() {
                        let resume_id = parsed
                            .get("resume_id")
                            .and_then(|v| v.as_str())
                            .unwrap_or("-");
                        format!("Upload successful (SmartJobs). resume_id: {}.", resume_id)
                    } else if let Some(data) = parsed.get("data").and_then(|d| d.as_object()) {
                        format!(
                            "Upload successful. ID: {}, file: {}",
                            data.get("id").and_then(|v| v.as_str()).unwrap_or("-"),
                            data.get("original_name")
                                .and_then(|v| v.as_str())
                                .unwrap_or(&file_name)
                        )
                    } else {
                        format!("Upload successful. Response: {}", body)
                    };
                    Ok(ToolResult {
                        success: true,
                        output: summary.into(),
                        error: None,
                    })
                }
                Err(e) => Ok(ToolResult {
                    success: false,
                    output: String::new().into(),
                    error: Some(format!("Request failed: {}.", e)),
                }),
            }
        }
    }
}
