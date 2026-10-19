//! 终端身份契约：slice 工具走网关时必须带 token，且下游记录要能归因到 `actor_sub`。
//!
//! 这是**手动集成测试**：需要一台已启动的终端（网关 + slice 服务 + 已签发的 token）。
//! 未设置 `JOBFIRST_SLICE_URL` 时直接跳过，避免 CI 依外部服务。
//!
//! ```text
//! JOBFIRST_SLICE_URL=http://127.0.0.1:3333/api/v1/slice \
//! JOBFIRST_SLICE_TOKEN=<HS256 JWT> \
//! cargo test -p zeroclaw-tools --test slice_identity -- --nocapture
//! ```

use serde_json::json;
use zeroclaw_api::tool::Tool;
use zeroclaw_tools::jobfirst_slice::JobFirstSliceSubmitResumeTool;

/// 网关地址未配置时跳过（本地联调用例，不进 CI 门禁）。
fn terminal_url() -> Option<String> {
    std::env::var("JOBFIRST_SLICE_URL")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[tokio::test]
async fn slice_tool_carries_identity_through_gateway() {
    let Some(url) = terminal_url() else {
        eprintln!("skip: JOBFIRST_SLICE_URL 未设置（非终端环境）");
        return;
    };
    assert!(
        std::env::var("JOBFIRST_SLICE_TOKEN")
            .map(|t| !t.trim().is_empty())
            .unwrap_or(false),
        "终端要求 JOBFIRST_SLICE_TOKEN；缺 token 时网关会 401，工具不应被当作可用"
    );

    let tool = JobFirstSliceSubmitResumeTool::new();
    let result = tool
        .execute(json!({ "resume_text": "Rust 后端工程师，5 年 tokio/axum" }))
        .await
        .expect("tool execute should not transport-error");

    assert!(
        result.success,
        "工具应成功，实际 error={:?}（url={}）",
        result.error, url
    );
    let out = result.output.to_string();
    let payload: serde_json::Value =
        serde_json::from_str(&out).expect("tool output should be JSON");
    let actor = payload
        .get("actor_sub")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    assert!(
        !actor.is_empty(),
        "下游必须记录 actor_sub（GFCC 1.3.0 归因），实际响应：{out}"
    );
    assert_ne!(
        actor, "anonymous",
        "actor_sub 不应是占位值 —— 那说明网关身份注入没生效"
    );
    eprintln!("ok: resume actor_sub={actor} url={url}");
}
