# JobFirst 技能包

## ⛔ 禁止行为（务必遵守）

- **严禁**在求职流程中调用 `proxy_config`、`model_routing_config`、`model_routing` 等配置类工具
- **严禁**调用 `web_search_tool` 来「找岗位」——岗位数据来自主仓 API，用 `jobfirst_get_matches`
- 上传简历后 **只允许** 调用 `jobfirst_get_matches`，不得调用 `web_search_tool` 或其他无关工具
- 若 API 失败，直接向用户说明，不尝试修改代理、路由或上网搜索

## Tatha 后端流程（助理 MVP）

### 用户提供简历文件路径时（PDF/DOCX/TXT）

- **必须使用 jobfirst_upload_resume**，勿使用 pdf_read、file_read 等通用工具
- jobfirst_upload_resume 会调用 Tatha /v1/documents/convert 解析并存入会话；之后 get_matches 可直接复用
- **upload_resume 成功后，立即调用 jobfirst_get_matches（无参数），勿调用 proxy_config 或其他无关工具**
- 若 upload_resume 失败（API 不可达等），直接向用户说明原因，不尝试 proxy_config
- 路径格式：直接提供本地绝对路径，如 `E:\Downloads\resume.pdf`；若在 ZIP 内，建议用户先解压再提供 PDF 路径

### 用户只说「帮我匹配岗位」「找匹配职位」时

1. 若会话内已有简历（用户此前已上传）→ 直接调用 jobfirst_get_matches（无需参数）
2. 若无简历 → 向用户索要：粘贴简历文本、或提供文件路径
3. 若用户提供文件路径 → 调用 jobfirst_upload_resume 上传解析，再调用 jobfirst_get_matches

### 工具选择

| 场景 | 使用工具 |
|------|----------|
| 用户给 PDF/DOCX/TXT 文件路径 | jobfirst_upload_resume（file_path） |
| 用户给 resume_text（粘贴） | jobfirst_get_matches（resume_text） |
| 用户给 .txt 文件路径 | jobfirst_upload_resume 或 jobfirst_get_matches（file_path）均可 |
| 已上传过简历，现要匹配 | jobfirst_get_matches（无参数） |

## 何时建议更新简历

- 用户提到「好久没投简历」「想换工作」时，建议先更新简历再查匹配
- 匹配度普遍偏低时，建议优化简历关键词或格式

## 如何解读匹配结果

- 优先展示得分高的职位，简要说明与简历的契合点
- 用户追问「为什么推荐这个」时，使用 `jobfirst_explain_match` 获取详细理由
- 若主仓尚未实现匹配/解释接口，说明当前能力边界，建议后续版本
