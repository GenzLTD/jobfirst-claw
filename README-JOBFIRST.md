# JobFirstClaw — 求职个人 AI 助理

> 基于 [ZeroClaw](https://github.com/zeroclaw-labs/zeroclaw) 的求职垂直扩展：对话式上传简历、职位匹配、匹配理由解读。

<p align="center">
  <strong>一个二进制 · 多通道入口 · 对接主仓 API</strong>
</p>

---

## 一、项目定位

JobFirstClaw 是 **ZeroClaw + 求职专用工具** 的单体项目，交付一个可用的「求职 AI 助理」：

| 维度 | 说明 |
|------|------|
| **底座** | ZeroClaw（Rust、单二进制、多通道） |
| **垂直能力** | 简历上传、职位匹配、匹配理由解读 |
| **入口** | CLI、Telegram、Webhook 等（按需开启） |
| **后端** | 对接 zervi-rust 主仓 API（Tatha / SmartJobs） |

**与 ZeroClaw 的关系**：JobFirstClaw = ZeroClaw + JobFirst 工具（addons）+ 求职助理身份与技能（workspace）。

---

## 二、核心能力

### 2.1 专用工具（M0 默认：Slice 收件箱）

`JOBFIRST_BACKEND=slice`（**默认**）时注册：

| 工具 | 职责 |
|------|------|
| `jobfirst_slice_submit_resume` | 提交简历 → 获得 `resume_id` |
| `jobfirst_slice_run_apply` | Agent 批量投递（Top-N 或指定 `job_ids`） |
| `jobfirst_slice_list_inbox` | 反馈收件箱 |
| `jobfirst_slice_inbox_decision` | 面试 / 跳过 |

环境变量：`JOBFIRST_SLICE_URL`（默认 `http://127.0.0.1:3333/api/v1/slice`，即经 api-gateway `:3333` 转发到 `jobfirst-slice-service`）。

> 直连请显式设为 `http://127.0.0.1:3005`。**仅本机联调使用** —— 直连会绕过网关侧鉴权与身份注入（`X-User-Id = sha256(email)`）。

**旧路径**（`JOBFIRST_BACKEND=tatha|smartjobs`）：`jobfirst_upload_resume` · `jobfirst_get_matches` · `jobfirst_explain_match`

### 2.2 身份与技能

- **Identity**：求职个人 AI 助理，能力边界、话术风格、工具选择约定
- **Skills**：何时用哪个工具、如何解读匹配结果、Tatha 与 SmartJobs 后端差异

### 2.3 后端支持

| 后端 | 环境变量 | 说明 |
|------|----------|------|
| **Tatha** | `JOBFIRST_API_URL` | 默认，调用 `/v1/documents/convert` 等 |
| **SmartJobs** | `JOBFIRST_BACKEND=smartjobs` | 调用主仓 Gateway `/api/v1/smartjobs` 等 |

---

## 三、快速开始

### 3.1 前置条件

- Rust 工具链（`rustup`）
- Windows：Visual Studio Build Tools（含 C++ 桌面开发）
- 主仓 API 已启动（zervi-rust 的 api-gateway）

### 3.2 初始化

```powershell
cd jobfirst-claw
.\setup.ps1
```

若 `setup.ps1` 报错「字符串缺少终止符」等编码问题，改用：`.\setup-run.ps1`

`setup.ps1` 会：

1. 克隆 ZeroClaw 到当前目录（若尚未存在）
2. 复制 JobFirst 工具到 `src/tools/`
3. 修改 `mod.rs` 注册工具
4. 复制 Identity 与 Skills 到 `~/.zeroclaw/workspace/`

### 3.3 环境变量

在项目根目录创建 `.env` 或配置环境变量：

```bash
# API 地址（必填）
JOBFIRST_API_URL=http://localhost:3333

# 认证 Token（upload-service 需 JWT，本地开发可生成测试 token）
# pip install pyjwt
# python scripts/gen_token.py
# 或运行: .\scripts\generate-dev-token.ps1
JOBFIRST_API_KEY=<生成的 JWT>

# 禁用 web_search（求职流程只用 jobfirst 工具，避免 Agent 误用 web 搜索）
WEB_SEARCH_ENABLED=false

# 或直接使用启动脚本（自动加载 .env 并禁用 web_search）：
# .\run-agent.ps1

# 后端类型（可选，默认 Tatha）
# JOBFIRST_BACKEND=tatha
# JOBFIRST_BACKEND=smartjobs

# SmartJobs 专用（可选）
# SMARTJOBS_API_URL=http://localhost:3333/api/v1/smartjobs

# 认证 Token（主仓需 JWT 时必填）
ZERVIGO_TOKEN=your_jwt_token
# 或
JOBFIRST_API_KEY=your_api_key
```

### 3.4 首次运行

```bash
# 1. 启动主仓 API（在 zervi-rust 根目录）
cargo run -p api-gateway

# 2. 初始化 ZeroClaw 配置（LLM Provider、API Key 等）
zeroclaw onboard --interactive

# 3. 对话测试
zeroclaw agent -m "帮我上传简历"
zeroclaw agent -m "E:\Downloads\resume.pdf 帮我匹配岗位"
```

### 3.5 开发模式

```bash
cargo build --release
cargo run -- agent -m "帮我匹配岗位"
```

---

## 四、目录结构

```
jobfirst-claw/
├── addons/                      # JobFirst 工具源码
│   ├── jobfirst_resume_store.rs
│   ├── jobfirst_upload_resume.rs
│   ├── jobfirst_get_matches.rs
│   └── jobfirst_explain_match.rs
├── workspace/                   # Identity + Skills 模板
│   ├── IDENTITY.md
│   └── skills/jobfirst/
├── setup.ps1                    # 初始化脚本
├── 设计思考与记录.md            # 架构与设计说明
└── README-JOBFIRST.md           # 本文档
```

---

## 五、与 ZeroClaw 的差异

| 维度 | ZeroClaw | JobFirstClaw |
|------|----------|--------------|
| 定位 | 通用 AI Agent 运行时 | 求职垂直 AI 助理 |
| 工具 | 通用（shell、file、web_search 等） | + 求职专用（upload_resume、get_matches、explain_match） |
| 身份 | 可配置，默认通用 | 预置「求职助理」身份与技能 |
| 后端 | 无业务 API 对接 | 对接 Tatha / SmartJobs / 主仓 Gateway |
| 路径策略 | 工作区路径限制 | 简历文件支持用户任意路径 |

---

## 六、主仓 API 约定

| 能力 | 方法 | 路径 |
|------|------|------|
| 上传简历 | POST | `/api/v1/upload/upload` |
| 匹配职位 | GET | `/api/v1/job/matches?resume_id=xxx&top_n=10` |
| 解释匹配 | GET | `/api/v1/job/{job_id}/explain?resume_id=xxx` |

主仓接口未就绪时，工具会返回友好占位说明，便于先跑通对话流程。

---

## 七、常见问题

### 7.1 简历路径报错

**勿用** `pdf_read`、`file_read` 处理简历——它们受工作区路径限制。必须使用 `jobfirst_upload_resume` 或 `jobfirst_get_matches` 的 `file_path` 参数。

### 7.2 主仓未启动

确认 `JOBFIRST_API_URL` 可达，且主仓 api-gateway 已运行。Tatha 默认端口 8010，SmartJobs 经 Gateway 通常为 3333。

### 7.3 认证失败

主仓需 JWT 时，设置 `ZERVIGO_TOKEN` 或 `JOBFIRST_API_KEY`，从 Zervigo 登录或主仓获取。

---

## 八、参考文档

- [设计思考与记录](设计思考与记录.md) — 架构决策与实现说明
- [API 参考](docs/api-reference.md) — 能力发现、Webhook、Tunnel、认证
- [JobFirst 个人 AI 助理版_ZeroClaw 路线](../JobFirst个人AI助理版_ZeroClaw路线.md) — 产品路线
- [ZeroClaw 官方仓库](https://github.com/zeroclaw-labs/zeroclaw)
- [ZeroClaw Channels Reference](https://github.com/zeroclaw-labs/zeroclaw/blob/main/docs/channels-reference.md) — 通道配置

---

## 九、致谢与归属

**本产品基于 [ZeroClaw](https://github.com/zeroclaw-labs/zeroclaw) 构建。** JobFirstClaw 并非 ZeroClaw 官方项目，亦未获 ZeroClaw Labs 背书。

- 底座：ZeroClaw（Copyright 2025 ZeroClaw Labs）
- 垂直扩展：JobFirst 求职工具与身份（见 [NOTICE](NOTICE)）

---

## 十、许可证

本扩展遵循 ZeroClaw 的 [MIT / Apache-2.0](LICENSE-MIT) 双许可。分发时请保留 `LICENSE-MIT`、`LICENSE-APACHE` 及 [NOTICE](NOTICE) 文件。
