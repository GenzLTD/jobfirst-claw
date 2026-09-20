# jobfirst-claw 最终定位 · 结论草案

| 项 | 内容 |
|---|---|
| **文件性质** | 结论草案（**非契约变更**，不改 `contracts/`） |
| **裁定人** | **@zervi-genz** |
| **起草** | 2026-09-20，基于 2026-09-19 ~ 09-20 的实证审计 |
| **依据** | GFCC 契约层 `contracts/registry.v1.json` · `docs/ports/README.md` §4 不变量 · `docs/vision.md` 五条判据 |
| **状态** | ⏳ **待裁定**：A 维持 ／ B 升格 ／ C 归档 |

> **落点说明**：本文件放在 `jobfirst-claw` 而非 `GenzLTD/docs`，因为 `docs` 仓的 `docs.yml` 在 push 到 main 时会跑 `mkdocs build --strict`，且 `mkdocs.yml` 有显式 `nav:` —— 新增未入 nav 的页面会使 strict 构建失败并给提交者发失败邮件。本仓 CI 已停驶，纯 commit 零运行。
> 若裁定为 B/C，建议按需要把本文档**镜像**进 `GenzLTD/docs`（届时需同步更新 `mkdocs.yml` 的 `nav`）。

---

## 1. 定位一句话

> **jobfirst-claw 不是格契的治理层，也不是任何治理范本；它是 `profile.demo` 下的一个辅助能力适配器 —— 且"可卸载、不进默认脊柱、不得作为共识面"这三条边界，契约早已表达过。**

## 2. 它在契约中的既存位置（无需新裁定，已有原文）

`contracts/registry.v1.json`：

```json
"optional_adapters": {
  "job_match": { "mode": "profile.demo", "may_feed": "identity.capability_trace", "weight": "auxiliary" }
},
"forbidden_in_default_tools": ["demopeter", "agentmemory", "iii", "job_match", ...]
```

`modes/gfcc.default.yaml` 的默认组合 = 内核 + `circle / identity / memory / consensus / contribution` —— **JobFirst 不在其中**。

`docs/ports/README.md` §2：「DemoPeter、agentmemory/iii、**job / poetry / credit / mbti 不得出现在默认清单**」；并对 `credit` 另注「**Must not be a consensus surface**」。

→ **契约已给出三条边界**：① 可卸载 ② 不进默认脊柱 ③ 输出只能作 `identity.capability_trace` 的**辅助**证据，**不得成为共识面**。

## 3. 资产盘点

| 侧 | 产出 | 性质 |
|---|---|---|
| 产品 | `jobfirst_slice_*` 四工具（提交简历 / 批量投递 / 反馈收件箱 / 面试跳过决策）、Tatha 与 SmartJobs 后端适配、identity + skills | 可交付能力，但依赖外部后端 |
| 工程 | ZeroClaw **v0.7.5** fork 基线，**与上游共享 git 历史** | 距上游 v0.8.5 **8 个 tag** |
| **治理** | **一次「外部熵注入压力测试」的实证** | ⭐ 本次审计的**唯一治理产出** |
| 流程 | 一条已验证的「扫描 → 建 issue → 回执」闭环 | 可复用原语 |

## 4. 事实结论（六条）

1. **CI 套件是外部继承物，不是治理范本**：34 个 workflow 文件中 **18 个非上游**、**16 个从未跑通**；9 个 active 里只有 2 个对本项目有用。
2. **组织层缺失是共性根因**：不存在的 runner（16 个文件）、不存在的 `scripts/` 目录（4 个文件）、不存在的分支（2 个文件）、不存在的 label（1 处）、指向他人域名的 CNAME（1 处）、被 `rust-toolchain.toml` 架空的 toolchain pin（1 处 —— 看起来有 pin、实际无 pin）。
3. **真正的 CI 从未运行过**：`Lint` 长期在 `cargo fmt` 失败即停，`build / check / test / security / bench` **全部被 skip**（13/13 次如此）。
4. 落后上游 8 个 tag；含 **2 个 yanked crate**（`chacha20`、`spin`）与 **5 条未 triage advisory**。
5. **白标/更名是合规必需**：仓内 `NOTICE` + `TRADEMARK.md` 明确禁止非官方仓库使用 ZeroClaw 名称；`CLA.md` / `CODEOWNERS` / `daily-audit.yml` 的 `cc @上游` 为待清残留。
6. **4 条漏洞防线**（漏洞告警 / Dependabot 安全更新 / Dependabot 版本更新 / advisory 扫描）原本**全断**，现已恢复 3 条（见第 6 节）。

## 5. 供裁定的三个选项

| 选项 | 内容 | 后果 |
|---|---|---|
| **A. 维持现状**（`profile.demo` 辅助画像） | 契约已表达"可卸载"；CI 已停驶；保留代码与历史；不新增投入 | 依赖漏洞不修（`Security` 作业未跑通）；落后上游继续拉大；无门控（依赖人工） |
| **B. 升格为独立产品立项** | 需产品侧判断：目标用户、变现路径、是否进入 90 天聚焦（对照 ADR 0009 / 0010 / 0011 的收敛口径） | 需恢复 CI 或建立替代门控；需补测试；需决定是否同步上游 v0.8.5 |
| **C. 降格 / 归档** | 从 `profile.demo` 移除引用；仓库归档（保留代码史） | **属契约变更，须走变更五步**：过 L-1 → 改 JSON → 改 `ports/README.md` → CHANGELOG → 适配器 |

## 6. 已执行的既成事实（不论裁定为何）

| 动作 | 状态 | 可逆 |
|---|---|---|
| **CI 停驶**：5 个 active CI workflow 全部 `disabled` | ✅ | ✅ `gh workflow enable` |
| 其余 11 个 workflow（7 个 Blacksmith + 3 个缺脚本 + 1 个） | ✅ 预先停用 | ✅ |
| 已注册 18 个 workflow → **16 disabled / 2 active** | ✅ | — |
| 仍 active 的 2 个：`Dependabot Updates`、`Dependency Graph` | ⏸️ **保留**（GitHub 托管的依赖更新引擎与依赖图，**非 CI**；按 owner 指示保留） | — |
| 新增 label `security` / `risk: high`；label 词表补齐至 26 个 | ✅ | — |
| 恢复漏洞告警 + Dependabot 安全更新 | ✅ | — |
| 清理 2 个孤儿分支；开启 `delete_branch_on_merge` / `allow_update_branch` | ✅ | — |
| `master` 已落地 fmt + clippy 修复（8 个提交，含 PR #20） | ✅ | — |
| 仓库订阅清零（`subscribers` 为空） | ✅ | — |

**已立案、待裁定后决定是否继续**：
`#21`（Blacksmith 方案 A/B）· `#25` / `#26` / `#27`（Security / Test / 32-bit 三个作业）· `#29`（labeler / CNAME / label 三处待修）

## 7. 裁定所需的最小决策（3 个问题）

1. **定位归属**：**A 维持 ／ B 升格 ／ C 归档**？（选 C 需走契约变更五步）
2. **上游策略**：同步 **v0.8.5** ／ 永久脱钩（则须自建 CVE 跟踪）？
3. **白标时点**：现在清理 ZeroClaw 名称残留（`CLA.md` / `CODEOWNERS` / `cc @上游`）／ 待定位确定后一并做？

## 8. 按 `vision.md` 四问自检（契约变更必答）

| 问 | 答 |
|---|---|
| 人还是目的吗？ | ✅ 是。求职工具服务于人，非替代人 |
| AI 有没有变成决策者？ | ✅ 没有。`job_match` 为 `weight: auxiliary`，仅作 `capability_trace` 辅助证据 |
| 治理权有没有被卖掉？ | ✅ 没有。**不得成为共识面**（与 `credit` 同规格约束） |
| 有结果是否仍不等于成功？ | ✅ 是。匹配结果 ≠ 共识结果 |

> **四问全过 → 它在契约内是安全的。但这不构成"应该继续投入"的理由 —— 那是产品判断，不是治理判断。**

---

## 附录 A：本文档的证据索引（issue 台账）

| # | 标题 | 状态 | 承载的结论 |
|---|---|---|---|
| #6 | 每日 CI 100% 空转：报警链路断裂 | CLOSED | 结论 2 / 3 / 6 |
| #15 | Quality Gate 13/13 全红 → 仅剩 3 作业 | OPEN（总览） | 结论 3 |
| #21 | 16 个 workflow 的 Blacksmith runner 不存在 | OPEN | 结论 2（runner） |
| #22 | CI 解锁 4 步清单 | OPEN | 结论 3 的执行过程 |
| #24 | `[security, risk: high]` 每日扫描自动 issue | OPEN | 结论 6（第 4 条防线已恢复） |
| #25 | Security 作业失败（2 yanked + 5 advisory + 13 条失效豁免） | OPEN | 结论 4 |
| #26 | Test 作业失败（2 个上游集成测试 API 迁移） | OPEN | 结论 3 |
| #27 | Check (32-bit)：`rust-toolchain.toml` 架空 toolchain pin | OPEN | 结论 2（pin） |
| #28 | 4 个 workflow 引用不存在的 `scripts/` | CLOSED | 结论 1 / 2 |
| #29 | labeler 缺规则 / docs CNAME / Sync Contributors label | OPEN | 结论 1 / 2 |

## 附录 B：本仓 workflow 最终状态（18 个已注册）

**active（2）** —— 均为 GitHub 托管、非 CI：
`Dependabot Updates`（`dynamic/dependabot/dependabot-updates`）· `Dependency Graph`（`dynamic/dependabot/update-graph`）

**disabled（16）**：
`Quality Gate` · `Daily Advisory Scan` · `PR Check Stale` · `PR Path Labeler` · `Validate PR title` · `Deploy mdBook docs to Pages` · `Sync Contributors` · `PR Auto Responder` · `PR Check Status` · `Workflow Sanity` · `Sec Audit` · `Sec CodeQL` · `Test Benchmarks` · `Test Fuzz` · `Feature Matrix` · `Pub Release`

（另有 `pr-labeler.yml` / `pr-intake-checks.yml` 从未注册 —— 触发条件 `branches: [dev, main]` 在本仓不存在）

---

## 裁定签署

- [ ] **A 维持** —— 保留下载画像现状，不新增投入
- [ ] **B 升格** —— 转独立产品立项（附产品判断依据）
- [ ] **C 归档** —— 移出 `profile.demo` 引用并归档仓库（**触发契约变更五步**）

裁定人：`@zervi-genz` ｜ 日期：____________ ｜ 备注：
