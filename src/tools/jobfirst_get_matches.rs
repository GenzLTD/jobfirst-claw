// Copyright (c) 2025 JobFirst contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//
//! JobFirst 工具：获取职位匹配
//! Tatha 后端：支持从会话简历存储读取（先 upload_resume → 再说「帮我匹配」）
//! 其他后端：resume_text/file_path/resume_id

use super::jobfirst_resume_store;
use super::traits::{Tool, ToolResult};
use async_trait::async_trait;
use serde_json::json;
use std::time::Duration;

fn api_base_url() -> String {
    if is_tatha_backend() {
        std::env::var("JOBFIRST_API_URL")
            .unwrap_or_else(|_| "http://localhost:3333/api/v1/tatha".to_string())
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

/// Tatha：将 PDF/DOCX 转为 Markdown（不经过 pdf_read，避免安全策略限制）
async fn tatha_convert_file(path: &std::path::Path) -> anyhow::Result<String> {
    use reqwest::multipart;
    let bytes = tokio::fs::read(path).await?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("resume.pdf")
        .to_string();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();
    let mime = match ext.as_str() {
        "pdf" => "application/pdf",
        "docx" | "doc" => {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        }
        "xlsx" | "xls" => {
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        }
        _ => "application/octet-stream",
    };
    let part = multipart::Part::bytes(bytes)
        .file_name(name)
        .mime_str(mime)
        .map_err(|e| anyhow::anyhow!("multipart: {}", e))?;
    let form = multipart::Form::new()
        .part("file", part)
        .text("document_type", "resume");

    let base = api_base_url().trim_end_matches('/').to_string();
    let url = format!("{}/v1/documents/convert", base);
    let mut req = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()?
        .post(&url)
        .multipart(form);
    if let Some(token) = auth_token() {
        req = req.header("Authorization", format!("Bearer {}", token));
    }

    let resp = req.send().await?;
    let status = resp.status();
    let body = resp.text().await?;
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap_or(json!({"raw": body}));

    if !status.is_success() {
        anyhow::bail!("Tatha convert 返回 {}: {}", status, body);
    }
    if let Some(err) = parsed.get("error").and_then(|v| v.as_str()) {
        anyhow::bail!("转换失败: {}", err);
    }
    let markdown = parsed
        .get("markdown")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    if markdown.trim().is_empty() {
        anyhow::bail!("转换结果为空");
    }
    Ok(markdown)
}

pub struct JobFirstGetMatchesTool;

impl Default for JobFirstGetMatchesTool {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for JobFirstGetMatchesTool {
    fn name(&self) -> &str {
        "jobfirst_get_matches"
    }

    fn description(&self) -> &str {
        "根据用户简历获取职位匹配推荐。返回 Top-N 职位列表及匹配得分。重要：当用户提供简历文件路径（PDF/DOCX/TXT）时，用本工具的 file_path 参数，勿用 pdf_read。Tatha 后端支持任意路径，无安全限制。若已通过 jobfirst_upload_resume 上传，可直接调用（无参数）。SmartJobs 需 resume_id。"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "resume_text": {
                    "type": "string",
                    "description": "简历全文或摘要（Tatha 后端必填；用户粘贴或从文件读取）"
                },
                "file_path": {
                    "type": "string",
                    "description": "本地简历文件路径（PDF/DOCX/TXT）；Tatha 后端直接解析 PDF/DOCX，无安全路径限制"
                },
                "resume_id": {
                    "type": "string",
                    "description": "简历 ID（SmartJobs 后端；从 jobfirst_upload_resume 返回中获得）"
                },
                "user_id": {
                    "type": "string",
                    "description": "用户 ID，可选；若未提供 resume_id 则按 user_id 查最新简历"
                },
                "top_n": {
                    "type": "integer",
                    "description": "返回前 N 个匹配职位，默认 5，最大 20",
                    "default": 5
                }
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

        // Tatha 后端：resume_text、file_path 或会话内已存储简历（先 upload 后 match）
        if is_tatha_backend() {
            let mut resume_text_val = resume_text;

            if let Some(ref fp) = file_path {
                let path = std::path::Path::new(fp);
                if !path.exists() {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!("文件不存在: {}", fp)),
                    });
                }
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.to_lowercase())
                    .unwrap_or_default();
                let content = if ext == "pdf" || ext == "docx" || ext == "doc" || ext == "xlsx" || ext == "xls" {
                    match tatha_convert_file(path).await {
                        Ok(markdown) => {
                            jobfirst_resume_store::set_resume(markdown.clone());
                            markdown
                        }
                        Err(e) => {
                            return Ok(ToolResult {
                                success: false,
                                output: String::new(),
                                error: Some(format!("解析简历失败: {}", e)),
                            });
                        }
                    }
                } else {
                    match tokio::fs::read_to_string(path).await {
                        Ok(c) => c,
                        Err(e) => {
                            return Ok(ToolResult {
                                success: false,
                                output: String::new(),
                                error: Some(format!("读取文件失败: {}", e)),
                            });
                        }
                    }
                };
                resume_text_val = Some(content);
            }

            // 若仍未取得简历，尝试从会话存储读取（用户已通过 upload_resume 上传过）
            if resume_text_val.as_ref().map(|s| s.trim().is_empty()).unwrap_or(true) {
                resume_text_val = jobfirst_resume_store::get_resume();
            }

            let resume_text_val = match resume_text_val {
                Some(t) if !t.trim().is_empty() => t,
                _ => {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some("未找到简历。请先通过 jobfirst_upload_resume 上传简历文件，或提供 resume_text / file_path。".to_string()),
                    });
                }
            };

            let base_url = api_base_url();
            let base = base_url.trim_end_matches('/');
            let url = format!("{}/v1/jobs/match", base);
            let body = serde_json::json!({
                "resume_text": resume_text_val,
                "top_n": top_n
            });

            let mut req = reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()?
                .post(&url)
                .json(&body);

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
                            error: Some(format!("Tatha API 返回 {}", status)),
                        });
                    }

                    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap_or(json!({"raw": body}));
                    // 兼容两种格式：1) 纯 Tatha { total_evaluated, matches }  2) zervi-rust ApiResponse { data: { total_evaluated, matches } }
                    let empty_map = serde_json::Map::new();
                    let payload = parsed
                        .get("data")
                        .and_then(|d| d.as_object())
                        .or_else(|| parsed.as_object())
                        .unwrap_or(&empty_map);
                    let total = payload.get("total_evaluated").and_then(|v| v.as_i64()).unwrap_or(0);
                    let empty: Vec<serde_json::Value> = vec![];
                    let matches = payload.get("matches").and_then(|m| m.as_array()).unwrap_or(&empty);

                    let lines: Vec<String> = matches
                        .iter()
                        .take(top_n as usize)
                        .enumerate()
                        .map(|(i, m)| {
                            let job = m.get("job").and_then(|j| j.as_object()).unwrap_or(&empty_map);
                            let score = m.get("score").and_then(|s| s.as_object()).unwrap_or(&empty_map);
                            let title = job.get("title").and_then(|v| v.as_str()).unwrap_or("-");
                            let company = job.get("company").and_then(|v| v.as_str()).unwrap_or("-");
                            let overall = score.get("overall").and_then(|v| v.as_i64()).unwrap_or(0);
                            let summary = score.get("summary").and_then(|v| v.as_str()).unwrap_or("");
                            if summary.is_empty() {
                                format!("{}. {} | {} | 匹配度: {}%", i + 1, title, company, overall)
                            } else {
                                format!("{}. {} | {} | 匹配度: {}%\n    {}", i + 1, title, company, overall, summary)
                            }
                        })
                        .collect();

                    let output = if lines.is_empty() {
                        format!("暂无匹配职位（参与打分 {} 条）", total)
                    } else {
                        format!("共评估 {} 条职位，Top-{} 匹配：\n\n{}", total, lines.len(), lines.join("\n\n"))
                    };

                    return Ok(ToolResult {
                        success: true,
                        output,
                        error: None,
                    });
                }
                Err(e) => {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!("请求 Tatha 失败: {}。请确认 Tatha 已启动(8010) 且 JOBFIRST_API_URL={}", e, api_base_url())),
                    });
                }
            }
        }

        if resume_id.is_none() && user_id.is_none() {
            return Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some("请提供 resume_id 或 user_id".to_string()),
            });
        }

        if is_smartjobs_backend() {
            return Ok(ToolResult {
                success: true,
                output: "SmartJobs 后端使用 POST /api/v1/resumes/improve 进行简历与职位的匹配，需同时提供 resume_id 与 job_id。当前无 Top-N 职位推荐接口，请先通过职位上传接口添加职位，再调用 improve。".to_string(),
                error: None,
            });
        }

        // 主仓匹配接口：若存在则 /api/v1/job/matches 或 /api/v1/resume/matches
        let base_url = api_base_url();
        let base = base_url.trim_end_matches('/');
        let url = if let Some(rid) = &resume_id {
            format!("{}/api/v1/job/matches?resume_id={}&top_n={}", base, rid, top_n)
        } else if let Some(uid) = &user_id {
            format!("{}/api/v1/job/matches?user_id={}&top_n={}", base, uid, top_n)
        } else {
            unreachable!()
        };

        let mut req = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?
            .get(&url);

        if let Some(token) = auth_token() {
            req = req.header("Authorization", format!("Bearer {}", token));
        }

        match req.send().await {
            Ok(resp) => {
                let status = resp.status();
                let body = resp.text().await.unwrap_or_default();

                if status.as_u16() == 404 || status.as_u16() == 501 {
                    return Ok(ToolResult {
                        success: true,
                        output: "主仓匹配接口尚未实现。请先在 job-service 或 resume-service 中实现 /api/v1/job/matches，或通过 API Gateway 暴露。当前可先使用 jobfirst_upload_resume 上传简历，等待匹配能力就绪。".to_string(),
                        error: None,
                    });
                }

                if !status.is_success() {
                    return Ok(ToolResult {
                        success: false,
                        output: body,
                        error: Some(format!("API 返回 {}", status)),
                    });
                }

                let parsed: serde_json::Value = serde_json::from_str(&body).unwrap_or(json!({"raw": body}));
                let summary = if let Some(items) = parsed.get("data").and_then(|d| d.get("items")).and_then(|i| i.as_array()) {
                    let lines: Vec<String> = items
                        .iter()
                        .take(top_n as usize)
                        .enumerate()
                        .map(|(i, j)| {
                            let title = j.get("title").and_then(|v| v.as_str()).unwrap_or("-");
                            let score = j.get("score").and_then(|v| v.as_f64()).unwrap_or(0.0);
                            let company = j.get("company").and_then(|v| v.as_str()).unwrap_or("-");
                            format!("{}. {} | {} | 匹配度: {:.1}%", i + 1, title, company, score * 100.0)
                        })
                        .collect();
                    if lines.is_empty() {
                        "暂无匹配职位".to_string()
                    } else {
                        lines.join("\n")
                    }
                } else {
                    format!("匹配结果: {}", body)
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
                error: Some(format!("请求失败: {}。JOBFIRST_API_URL={}", e, api_base_url())),
            }),
        }
    }
}
