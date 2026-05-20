// Copyright (c) 2025 JobFirst contributors
// SPDX-License-Identifier: MIT OR Apache-2.0
//
//! JobFirst 工具：解释匹配理由
//! 针对某职位 ID 返回匹配理由（技能契合、经历匹配等）

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

pub struct JobFirstExplainMatchTool;

impl Default for JobFirstExplainMatchTool {
    fn default() -> Self {
        Self
    }
}

#[async_trait]
impl Tool for JobFirstExplainMatchTool {
    fn name(&self) -> &str {
        "jobfirst_explain_match"
    }

    fn description(&self) -> &str {
        "针对某个职位返回匹配理由。Tatha 后端：提供 job_index（第几个匹配，从 1 开始）和 resume_text/file_path；SmartJobs 后端：提供 job_id 和 resume_id。"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "job_index": {
                    "type": "integer",
                    "description": "第几个匹配职位（从 1 开始，Tatha 后端用）"
                },
                "job_id": {
                    "type": "string",
                    "description": "职位 ID（SmartJobs 后端）"
                },
                "resume_text": {
                    "type": "string",
                    "description": "简历全文（Tatha 后端必填）"
                },
                "file_path": {
                    "type": "string",
                    "description": "本地简历文件路径（Tatha 后端，有则读取作为 resume_text）"
                },
                "resume_id": {
                    "type": "string",
                    "description": "简历 ID（SmartJobs 后端）"
                }
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

        // Tatha 后端：job_index + resume_text 或 file_path
        if is_tatha_backend() {
            let mut resume_text_val = resume_text;

            if let Some(ref fp) = file_path {
                let path = std::path::Path::new(fp);
                if path.exists() {
                    match tokio::fs::read_to_string(path).await {
                        Ok(content) => resume_text_val = Some(content),
                        Err(e) => {
                            return Ok(ToolResult {
                                success: false,
                                output: String::new(),
                                error: Some(format!("读取文件失败: {}", e)),
                            });
                        }
                    }
                } else {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!("文件不存在: {}", fp)),
                    });
                }
            }

            let resume_text_val = match resume_text_val {
                Some(t) if !t.trim().is_empty() => t,
                _ => {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some("Tatha 后端需要 resume_text 或 file_path。请提供简历内容或文件路径。".to_string()),
                    });
                }
            };

            let base_url = api_base_url();
            let base = base_url.trim_end_matches('/');
            let url = format!("{}/v1/jobs/match", base);
            let body = serde_json::json!({
                "resume_text": resume_text_val,
                "top_n": 20
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

                    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap_or(json!({}));
                    let empty: Vec<serde_json::Value> = vec![];
                    let matches = parsed.get("matches").and_then(|m| m.as_array()).unwrap_or(&empty);
                    let empty_map = serde_json::Map::new();

                    let idx = job_index.saturating_sub(1);
                    let Some(m) = matches.get(idx) else {
                        return Ok(ToolResult {
                            success: true,
                            output: format!("第 {} 个匹配不存在，共 {} 条结果。请先调用 jobfirst_get_matches 获取匹配列表。", job_index, matches.len()),
                            error: None,
                        });
                    };

                    let score = m.get("score").and_then(|s| s.as_object()).unwrap_or(&empty_map);
                    let job = m.get("job").and_then(|j| j.as_object()).unwrap_or(&empty_map);
                    let title = job.get("title").and_then(|v| v.as_str()).unwrap_or("-");
                    let company = job.get("company").and_then(|v| v.as_str()).unwrap_or("-");
                    let overall = score.get("overall").and_then(|v| v.as_i64()).unwrap_or(0);
                    let summary = score.get("summary").and_then(|v| v.as_str()).unwrap_or("");
                    let fit_bullets = score.get("fit_bullets").and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(String::from).collect::<Vec<_>>())
                        .unwrap_or_default();

                    let mut lines = vec![
                        format!("【{} | {}】匹配度: {}%", title, company, overall),
                        String::new(),
                        summary.to_string(),
                    ];
                    if !fit_bullets.is_empty() {
                        lines.push(String::new());
                        lines.push("匹配要点：".to_string());
                        for b in &fit_bullets {
                            lines.push(format!("• {}", b));
                        }
                    }

                    return Ok(ToolResult {
                        success: true,
                        output: lines.join("\n"),
                        error: None,
                    });
                }
                Err(e) => {
                    return Ok(ToolResult {
                        success: false,
                        output: String::new(),
                        error: Some(format!("请求 Tatha 失败: {}", e)),
                    });
                }
            }
        }

        // SmartJobs 后端需要 job_id
        let job_id = match job_id {
            Some(id) if !id.is_empty() => id,
            _ => {
                return Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some("缺少 job_id 参数".to_string()),
                });
            }
        };

        if is_smartjobs_backend() {
            let rid = resume_id.unwrap_or("");
            if rid.is_empty() {
                return Ok(ToolResult {
                    success: true,
                    output: "SmartJobs 后端使用 POST /api/v1/resumes/improve 获取匹配分析，需同时提供 resume_id 与 job_id。请提供 resume_id 后重试。".to_string(),
                    error: None,
                });
            }
            let base_url = api_base_url();
            let base = base_url.trim_end_matches('/');
            let url = format!("{}/resumes/improve", base);
            let body = serde_json::json!({ "resume_id": rid, "job_id": job_id });
            let mut req = reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()?
                .post(&url)
                .json(&body);
            if let Some(token) = auth_token() {
                req = req.header("Authorization", format!("Bearer {}", token));
            }
            return match req.send().await {
                Ok(resp) => {
                    let status = resp.status();
                    let txt = resp.text().await.unwrap_or_default();
                    if !status.is_success() {
                        Ok(ToolResult {
                            success: false,
                            output: txt,
                            error: Some(format!("API 返回 {}", status)),
                        })
                    } else {
                        let parsed: serde_json::Value = serde_json::from_str(&txt).unwrap_or(json!({"raw": txt}));
                        let summary = parsed.get("data").and_then(|d| d.as_object()).map(|d| {
                            let new_score = d.get("new_score").and_then(|v| v.as_f64())
                                .or_else(|| d.get("updated_score").and_then(|v| v.as_f64()))
                                .unwrap_or(0.0);
                            let orig = d.get("original_score").and_then(|v| v.as_f64()).unwrap_or(0.0);
                            let resume = d.get("updated_resume").and_then(|v| v.as_str()).unwrap_or("");
                            let preview = if resume.len() > 800 { format!("{}...", &resume[..800]) } else { resume.to_string() };
                            format!("原始匹配分: {:.1}% → 改进后: {:.1}%\n\n改进建议/更新简历:\n{}", orig * 100.0, new_score * 100.0, preview)
                        }).unwrap_or_else(|| format!("匹配结果: {}", txt));
                        Ok(ToolResult {
                            success: true,
                            output: summary,
                            error: None,
                        })
                    }
                }
                Err(e) => Ok(ToolResult {
                    success: false,
                    output: String::new(),
                    error: Some(format!("请求失败: {}", e)),
                }),
            };
        }

        let base_url = api_base_url();
        let base = base_url.trim_end_matches('/');
        let url = match resume_id {
            Some(rid) => format!("{}/api/v1/job/{}/explain?resume_id={}", base, job_id, rid),
            None => format!("{}/api/v1/job/{}/explain", base, job_id),
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
                        output: format!(
                            "主仓「匹配解释」接口尚未实现。职位 ID: {}。\
                            请先在 job-service 中实现 GET /api/v1/job/{{job_id}}/explain，\
                            返回技能契合度、经历匹配点等结构化说明。",
                            job_id
                        ),
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
                let explanation = parsed
                    .get("data")
                    .and_then(|d| d.get("explanation").and_then(|v| v.as_str()))
                    .unwrap_or_else(|| parsed.get("data").and_then(|d| d.as_str()).unwrap_or(&body));

                Ok(ToolResult {
                    success: true,
                    output: explanation.to_string(),
                    error: None,
                })
            }
            Err(e) => Ok(ToolResult {
                success: false,
                output: String::new(),
                error: Some(format!("请求失败: {}", e)),
            }),
        }
    }
}
