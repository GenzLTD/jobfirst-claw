// Copyright (c) 2025 JobFirst contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//
//! JobFirst 工具：上传简历并解析
//! Tatha 后端：调用 /v1/documents/convert 转 Markdown，存入会话简历存储（先上传→再匹配）
//! 主仓/SmartJobs：调用上传 API

use super::jobfirst_resume_store;
use super::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::time::Duration;

fn is_tatha_backend() -> bool {
    std::env::var("JOBFIRST_BACKEND")
        .map(|v| v.to_lowercase() == "tatha")
        .unwrap_or(false)
}

fn api_base_url() -> String {
    if is_smartjobs_backend() {
        std::env::var("SMARTJOBS_API_URL")
            .or_else(|_| std::env::var("JOBFIRST_API_URL"))
            .unwrap_or_else(|_| "http://47.115.168.107:9001/api/v1".to_string())
    } else {
        std::env::var("JOBFIRST_API_URL")
            .unwrap_or_else(|_| "http://localhost:3333".to_string())
    }
}

/// 是否为 SmartJobs 后端（路径与响应格式不同）
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

/// 规范化 LLM 可能传错的路径（如 \Users\... 或 \\Users\\... 缺盘符）
#[cfg(windows)]
fn normalize_file_path(s: &str) -> String {
    let s = s.trim().replace('/', "\\");
    let s = s.trim_start_matches('\"').trim_end_matches('\"');
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
    s.trim().trim_start_matches('\"').trim_end_matches('\"').to_string()
}

pub struct JobFirstUploadResumeTool {
    /// 是否已校验过环境
    _placeholder: (),
}

impl Default for JobFirstUploadResumeTool {
    fn default() -> Self {
        Self { _placeholder: () }
    }
}

#[async_trait]
impl Tool for JobFirstUploadResumeTool {
    fn name(&self) -> &str {
        "jobfirst_upload_resume"
    }

    fn description(&self) -> &str {
        "上传简历文件到 JobFirst 并解析。支持 PDF、DOCX、TXT。重要：处理简历文件时用本工具，勿用 pdf_read（本工具无路径限制）。需要 file_path 参数（本地绝对路径如 C:\\Users\\xxx\\resume.pdf）。Tatha 后端会存入会话，之后可直接说「帮我匹配」。"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "本地简历文件的完整路径，如 /path/to/resume.pdf"
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
                output: String::new(),
                error: Some("缺少 file_path 参数".to_string()),
            });
        }

        let path = std::path::Path::new(&file_path);
        if !path.exists() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("文件不存在: {}", file_path)),
            });
        }

        let bytes = tokio::fs::read(path).await.map_err(|e| {
            anyhow::anyhow!("读取文件失败: {}", e)
        })?;

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("resume.pdf")
            .to_string();

        // Tatha 后端：调用 /v1/documents/convert → 存入会话简历 → 用户后续可直接说「帮我匹配」
        if is_tatha_backend() {
            let base_url = api_base_url();
            let base = base_url.trim_end_matches('/');
            let url = format!("{}/v1/documents/convert", base);
            let part = reqwest::multipart::Part::bytes(bytes)
                .file_name(file_name.clone())
                .mime_str(
                    path.extension()
                        .and_then(|e| e.to_str())
                        .map(|e| match e.to_lowercase().as_str() {
                            "pdf" => "application/pdf",
                            "docx" | "doc" => {
                                "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                            }
                            "xlsx" | "xls" => {
                                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
                            }
                            "txt" => "text/plain",
                            _ => "application/octet-stream",
                        })
                        .unwrap_or("application/octet-stream"),
                )
                .map_err(|e| anyhow::anyhow!("multipart: {}", e))?;
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
                            output: body,
                            error: Some(format!("Tatha convert API 返回 {}", status)),
                        });
                    }

                    let parsed: serde_json::Value =
                        serde_json::from_str(&body).unwrap_or(json!({"raw": body}));
                    let markdown = parsed
                        .get("markdown")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    if let Some(ref err) = parsed.get("error").and_then(|v| v.as_str()) {
                        return Ok(ToolResult {
                            success: false,
                            output: body,
                            error: Some(format!("转换失败: {}", err)),
                        });
                    }

                    if markdown.trim().is_empty() {
                        return Ok(ToolResult {
                            success: false,
                            output: body,
                            error: Some("转换结果为空".to_string()),
                        });
                    }

                    jobfirst_resume_store::set_resume(markdown.clone());
                    let preview = if markdown.len() > 200 {
                        format!("{}...", &markdown[..200])
                    } else {
                        markdown.clone()
                    };
                    return Ok(ToolResult {
                        success: true,
                        output: format!(
                            "简历已上传并解析。已存入会话，可直接说「帮我匹配岗位」获取推荐。\n解析预览：\n{}",
                            preview
                        ),
                        error: None,
                    });
                }
                Err(e) => {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!(
                            "Tatha convert 请求失败: {}。请确认 JOBFIRST_API_URL({}) 可达且 Tatha 已启动。",
                            e,
                            api_base_url()
                        )),
                    });
                }
            }
        }

        let base_url = api_base_url();
        let base = base_url.trim_end_matches('/');
        let url = if is_smartjobs_backend() {
            format!("{}/resumes/upload", base)
        } else {
            format!("{}/api/v1/upload/upload", base)
        };
        let part = reqwest::multipart::Part::bytes(bytes)
            .file_name(file_name.clone())
            .mime_str(
                path.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| match e.to_lowercase().as_str() {
                        "pdf" => "application/pdf",
                        "docx" | "doc" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                        _ => "application/octet-stream",
                    })
                    .unwrap_or("application/octet-stream"),
            )
            .map_err(|e| anyhow::anyhow!("multipart: {}", e))?;

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
                    let err_msg = format!("API 返回 {}: {}", status, &body);
                    return Ok(ToolResult {
                        success: false,
                        output: body,
                        error: Some(err_msg),
                    });
                }

                let parsed: serde_json::Value = serde_json::from_str(&body).unwrap_or(json!({"raw": body}));
                let summary = if is_smartjobs_backend() {
                    let resume_id = parsed.get("resume_id").and_then(|v| v.as_str()).unwrap_or("-");
                    let info = parsed.get("info").and_then(|v| v.as_str()).unwrap_or("");
                    format!(
                        "上传成功 (SmartJobs)。resume_id: {}。{}",
                        resume_id,
                        if info.is_empty() { "可用 GET /api/v1/resumes?resume_id=xxx 查询解析状态" } else { info }
                    )
                } else if let Some(data) = parsed.get("data").and_then(|d| d.as_object()) {
                    format!(
                        "上传成功。ID: {}，文件名: {}，分类: {:?}",
                        data.get("id").and_then(|v| v.as_str()).unwrap_or("-"),
                        data.get("original_name").and_then(|v| v.as_str()).unwrap_or(&file_name),
                        data.get("classification")
                    )
                } else {
                    format!("上传成功。响应: {}", body)
                };

                Ok(ToolResult {
                    success: true,
                    output: summary,
                    error: None,
                })
            }
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("请求失败: {}。请确认 JOBFIRST_API_URL({}) 可达。", e, api_base_url())),
            }),
        }
    }
}
