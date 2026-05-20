# JobFirstClaw API 参考

> 简要 API 文档，供编排器与外部系统对接使用。

## 认证

所有 `/api/*` 接口需 Bearer Token：

```
Authorization: Bearer <token>
```

**获取 Token 方式：**

1. **配对**：`POST /pair`，头 `X-Pairing-Code`（首次启动时终端显示）
2. **环境变量预配**：设置 `ZEROCLAW_API_KEY` 或 `JOBFIRST_API_KEY`，启动时自动加入已配对 Token，便于 CI/编排器接入

---

## 能力发现

### GET /api/capabilities

返回 Agent 能力概览，供编排器发现与路由。

**响应示例：**

```json
{
  "agent_type": "jobfirst",
  "version": "0.1.7",
  "tools": [
    { "name": "jobfirst_upload_resume", "description": "..." },
    { "name": "jobfirst_get_matches", "description": "..." },
    { "name": "jobfirst_explain_match", "description": "..." }
  ],
  "channels": { "telegram": true, "webhook": true },
  "sops": [
    { "name": "deploy-pipeline", "description": "...", "triggers": ["webhook:/sop/deploy"] }
  ],
  "supported_triggers": ["webhook", "sop_webhook", "mqtt", "cron"],
  "endpoints": {
    "webhook": "POST /webhook",
    "sop": "POST /sop/*",
    "health": "GET /health",
    "metrics": "GET /metrics"
  }
}
```

---

## 核心端点

| 方法 | 路径 | 说明 |
|------|------|------|
| GET | `/health` | 健康检查（无需鉴权） |
| POST | `/pair` | 配对，获取 Bearer Token |
| POST | `/webhook` | 发送消息：`{"message":"..."}` |
| POST | `/sop/{path}` | 触发 SOP，路径与 SOP.toml 中的 `path` 精确匹配 |
| GET | `/metrics` | Prometheus 指标 |
| GET | `/api/status` | 系统状态 |
| GET | `/api/tools` | 工具列表（含参数） |
| GET | `/api/capabilities` | 能力概览 |
| GET | `/api/health` | 组件健康快照 |

---

## Webhook

```bash
curl -X POST http://127.0.0.1:42617/webhook \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"message": "帮我匹配岗位"}'
```

**可选 Header：**

- `X-Idempotency-Key`：防重复处理
- `X-Webhook-Secret`：若配置，作二次验证
- **`X-Callback-URL`**：异步模式。立即返回 `202 Accepted`，后台处理完成后将结果 `POST` 到该 URL。回调体：`{"status":"completed","response":"..."}` 或 `{"status":"failed","error":"..."}`

---

## SOP Webhook

```bash
curl -X POST http://127.0.0.1:42617/sop/deploy \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{"message":"deploy-service-a"}'
```

---

## 公网暴露（Tunnel）

如需外部系统访问，需配置 tunnel 或显式允许公网绑定。

### config.toml 示例

```toml
[tunnel]
provider = "tailscale"   # 或 "ngrok", "cloudflare"

[gateway]
host = "127.0.0.1"
port = 42617
allow_public_bind = false  # 建议 false，通过 tunnel 暴露
```

启动后 tunnel 会输出公网 URL，例如：

```
🌐 Public URL: https://xxx.tailscale.net
```

将该 URL 配置到调用方（如 `https://xxx.tailscale.net/webhook`）。
