# DesktopAide（桌面助手）

> Windows 桌面 AI 助手：以浮动气泡常驻桌面，点开即用。多模型对话、联网搜索、定时提醒、截图识图、图片生成，聊天记录与图片全部保存在本地。

<p align="center">
  <em>常驻气泡 · 多模型 · 本地存储 · 无需账号</em>
</p>

## 目录

- [特性](#特性)
- [环境要求](#环境要求)
- [快速开始](#快速开始)
- [首次配置](#首次配置)
- [架构说明](#架构说明)
- [配置项说明](#配置项说明)
- [数据与隐私](#数据与隐私)
- [快捷键与斜杠命令](#快捷键与斜杠命令)
- [常见问题](#常见问题)
- [已知限制](#已知限制)
- [许可证](#许可证)

## 特性

- **浮动气泡**：置顶常驻、可拖动定位；前台窗口最大化时自动收折为屏幕边缘细条，悬停恢复；支持定时自动收折（3 秒 ~ 24 小时）
- **多模型对话**：Anthropic、OpenAI、xAI/Grok、Gemini、DeepSeek、通义千问、Kimi、智谱、OpenRouter、Ollama，以及任何 OpenAI Chat Completions 兼容服务（自定义网关）；SSE 流式输出
- **联网搜索**：开启后 AI 在需要最新信息时自动检索网页并附来源链接。多源级联：Tavily（可选 Key）→ 必应（RSS 端点，免 Key）→ 百度（免 Key）→ DuckDuckGo（免 Key），单源失败自动换下一个，并对瞬时网络错误重试一次
- **工具调用**：内置 ReAct 工具闭环（最多 8 轮）——获取当前时间、创建/查看/取消定时提醒、联网搜索；工具执行全程在 Rust 侧，前端只做展示
- **定时提醒**：自然语言时间（「10 分钟后」「2 小时后」「14:30」）；到期以气泡脉冲 + 面板内助手消息呈现（不依赖系统通知）；支持查看/取消，应用重启后自动恢复，关闭期间到期的会在启动时补发
- **多模态**：图片附件（粘贴/选择/截图）。**识图与出图是两个独立开关（各二选一）**：识图默认关闭（用本地 OCR 把图中文字识别出来注入上下文，兼容性最好），模型支持识图（GPT-4o、Claude、Gemini、DeepSeek V4.1 等）时切到「把图片发给模型」即可获得图像理解；出图默认关闭，使用的模型是图片生成模型（grok-2-image、gpt-image、dall-e、imagen 等）时切到「是出图模型」，生成的图片会落盘持久化。同一模型两者都支持时（如 `gemini-2.5-flash-image`）两个开关都打开
- **文本文件附件**：txt/md/代码等（≤512 KB，超长自动截断）注入模型上下文
- **对话管理**：SQLite 持久化多会话；自动标题、重命名、删除、切换；消息撤回（可撤销）；清空对话
- **内容渲染**：Markdown、GFM、LaTeX（KaTeX）、代码块高亮、图片灯箱（可下载/保存）
- **主题定制**：深浅色主题预设 + 强调色/背景色/背景透明度/用户气泡配色微调
- **数据管理**：配置/聊天记录/图片统一存放，支持一键迁移数据目录（复制迁移 + 自动重启）；托盘菜单与退出确认
- **截图提问**：`Ctrl + Alt + C` 全屏截图选框，直接作为图片附件提问

## 环境要求

### 运行（使用打包后的安装包）

| 项 | 要求 |
|---|---|
| 操作系统 | Windows 10 1809+ / Windows 11（x64） |
| WebView2 Runtime | Windows 11 已预装；Windows 10 需安装（[下载](https://developer.microsoft.com/microsoft-edge/webview2/)） |
| 磁盘空间 | 安装后约 120 MB（含 91 MB OCR 引擎与模型）+ 聊天数据 |
| 权限 | 无需管理员权限（默认按当前用户安装） |

### 开发（从源码构建）

| 依赖 | 版本 / 说明 |
|---|---|
| Node.js | **≥ 20 LTS**（含 npm） |
| Rust | **≥ 1.77.2 stable**，工具链 `x86_64-pc-windows-msvc` |
| C++ 构建工具 | **Visual Studio 2022 生成工具**，勾选「使用 C++ 的桌面开发」（提供 MSVC 链接器与 Windows SDK，`windows-rs` / `webview2-com` 编译必需） |
| WebView2 Runtime | 开发运行同样需要 |
| 磁盘 | 建议预留 **≥ 10 GB**（Rust `target/` 增量产物较大；本项目实测单次 `cargo clean` 可释放 25 GB 陈旧产物） |
| 网络 | 首次 `cargo build` 需访问 crates.io 拉取依赖；打包时 Tauri 会下载 NSIS/WiX（离线可加 `--bundles nsis` 复用 `%LOCALAPPDATA%\tauri\NSIS`） |

> 说明：`resources/` 目录（91 MB，RapidOCR 可执行文件 + ONNX 模型）随仓库提供，无需额外下载。

## 快速开始

```bash
# 1. 安装前端依赖
npm install

# 2. 启动开发模式（Vite :5173 + Tauri 原生窗口）
npm run tauri dev
```

首次启动会编译全部 Rust 依赖并链接，耗时较长；之后为增量编译。

**日志位置（仅 debug 构建写入）**：`%LOCALAPPDATA%\com.desktop-aide.app\logs\DesktopAide.log`

```bash
# 仅调试前端（浏览器打开，不含原生能力）
npm run dev
```

## 首次配置

1. **打开面板**：点击桌面气泡，或按 `Ctrl + Alt + Space`
2. **设置 → AI 服务**
   - 选择服务商；填写**模型名**
   - 非 Ollama 服务商填 **API Key**；Ollama 填地址；`自定义兼容服务` 需填兼容 API 地址（如 `https://your-gateway/v1`）
   - **图片输入方式（识图能力）**：二选一，默认「OCR 识别文字」；若所用模型支持识图（GPT-4o / Claude / Gemini / DeepSeek V4.1 等），改为「把图片发给模型」以获得更好的图像理解
   - **模型用途（出图能力）**：二选一，默认「不是出图模型」；若所用模型是图片生成模型（`gpt-image`、`dall-e`、`imagen`、`gemini-*-image` 等），改为「是出图模型」。**它只改变传输方式（走非流式），不再禁用工具/联网搜索**，因此两个开关可以同时打开
   - 「高级参数」可调 API 地址、Temperature、Max Tokens
3. **设置 → 通用**
   - **AI 能力 → 启用工具调用**：允许 AI 调用时间/提醒工具
   - **AI 能力 → 允许 AI 联网搜索**：允许 AI 按需联网；可填 **Tavily API Key**（可选，留空则使用必应/百度等免 Key 源）
   - **气泡行为**：关闭面板后自动收折的时间（3 秒 ~ 24 小时）
   - **存储位置**：查看当前数据目录，或迁移到其它磁盘（复制迁移，完成后自动重启）
   - **数据管理**：清除所有对话数据（不可撤销，需二次确认）
4. **设置 → 外观**：主题预设、强调色、背景色、背景透明度
5. 点击面板底部 **保存配置**
6. 开始使用：`Ctrl + V` 粘贴图片/文本文件，`Ctrl + Alt + C` 截图提问，输入 `/help` 查看命令

## 架构说明

### 前端

- **状态**：4 个 Zustand store —— `configStore`（配置 + `normalizeConfig()` 校验/钳制）、`sessionStore`（消息、流式文本、附件、对话列表、灯箱）、`uiStore`（面板、设置、气泡收折、退出确认）、`reminderStore`（面板内提醒卡片）
- **数据流**：所有 Tauri 调用集中在 `services/tauriApi.ts`，组件不直接依赖 command 名称
- **流式渲染**：SSE chunk 按 `requestId` 行缓冲，完整行立即 flush，尾部由 100ms 定时器补齐（定时器不被后续 chunk 推迟，避免「文本停住后猛跳」）；64 KB/条与 10 条缓冲上限防内存泄漏；`StreamingBubble` 自订阅流式文本，每个 chunk 只重渲染该组件
- **业务逻辑**：`useChatActions` 集中聊天/命令/附件逻辑，store 读取用 `getState()` 避免 useCallback 引用漂移
- **图片引用**：后端持久化 `imgref://{id}` 标记，前端 `lib/localImages.ts` 通过 `convertFileSrc` 转为受 asset scope 保护的本地 URL，保存时反向还原
- **异常兜底**：`ErrorBoundary` 包裹应用根，渲染异常不再整窗白屏

### 后端

- **AI Provider**：`AiProvider` 枚举分发（非 trait object）——Anthropic 走 `/messages` + `x-api-key`，其余全部走 OpenAI 兼容 `/chat/completions` + Bearer；`from_config()` 解析 API 地址（Ollama 默认 `localhost:11434`，Gemini 走兼容端点，自定义服务用 `api_base`）
- **Agent**：单步对话（`run` / `run_with_images`）与工具闭环（`run_with_tools`，ReAct 最多 8 轮，工具串行执行，失败结果回传不中断对话）；上下文 `ContextManager` 采用 CJK 感知 token 估算（图片 base64 按 1/8 折算），默认保留 20 轮 / 16000 token
- **联网搜索**：`web_search.rs` 多源级联，单源超时 `connect 5s / read 10s / 总 30s`，瞬时网络错误重试一次；必应优先使用 **RSS 端点**（响应约 4 KB，避免大页面被读取超时截断）
- **多模态**：「识图」（能否看图）与「出图」（能否画图）是两个**独立的二选一开关**。识图由 `vision_mode` 决定（`off` = OCR 回退，`on` = 按多模态发送原图）。出图由 `model_kind` 决定（`chat` / `image`）。出图模型走非流式（图片拿不到增量 delta），但**仍可携带 `tools`**：`Agent::run_with_tools` 接受 `stream` 参数在流式/非流式之间切换，非流式首轮若被服务端以「不支持 tools」拒绝会自动摘掉工具重试一次。图片生成响应兼容 6+ 种格式（image_url / b64_json / venus_multimodal_url / inline_data / message.images / source 等）。仅专用出图端点（xAI `/images/generations`，只接受 prompt 字符串）无法携带工具
- **持久化**：`save_message` 把 content 中的 `data:image` 与附件 base64 提取为文件（`ImageStore`），数据库存 `imgref://{id}` 与 `images` 表引用；**消息 + 图片引用 + 会话时间戳 + 自动标题在同一事务内提交**
- **并发与取消**：`agent_busy` 原子 CAS 拒绝并发请求，并由 RAII 守卫保证异常时也会复位；`ai_cancel` oneshot 支持「停止生成」并保留已生成内容
- **提醒调度**：以 `id → JoinHandle` 注册表管理等待任务，取消时 `abort`；触发前用「删除数据库行」原子认领，避免取消后仍触发；启动时对已到期提醒补发
- **数据目录**：`bootstrap.json` 记录自定义数据目录；迁移 = 校验 → 写探针 → WAL checkpoint 后复制 DB → 复制 images/config → 写注册表 → 重启

### 关键数据流

1. **聊天**：`useChatActions.handleSend` → `invoke("ai_chat")` → `Agent.run*()` → SSE 流式 → 每个 chunk 发 `ai-stream-chunk` → 前端行缓冲渲染 → `ai-stream-done` → `commitStreamToMessage()` → 持久化到 SQLite
2. **工具调用**：模型返回 `tool_calls` → `execute_tool()`（Rust 侧）→ 结果作为 `tool` 消息回传 → 进入下一轮，直到无工具调用或达到 8 轮上限
3. **图片附件**：粘贴/选择 → 压缩 → 按视觉能力分流（多模态直发 / OCR 回退注入文本）→ 附件与生成图落盘为文件 → 数据库存引用
4. **对话切换**：`switch_conversation` → 加载消息与图片引用 → 路径还原 base64 → `rebuild_context_from_pairs` 重建 Agent 记忆

## 配置项说明

配置文件：`%APPDATA%\DesktopAide\config.json`（迁移后位于自定义数据目录）。

| 字段 | 说明 |
|---|---|
| `ai_provider` | `anthropic` / `openai` / `xai` / `gemini` / `deepseek` / `qwen` / `kimi` / `zhipu` / `openrouter` / `ollama` / `custom` |
| `model`、`api_key`、`api_base` | 模型名、密钥、兼容 API 地址（自定义服务必填） |
| `max_tokens`、`temperature` | 采样参数 |
| `ollama_endpoint`、`ollama_model` | Ollama 地址与模型名 |
| `enable_tools` | 工具调用总开关（时间 / 提醒） |
| `enable_web_search` | 允许 AI 联网搜索 |
| `tavily_api_key` | 可选 Tavily Key；留空时使用免 Key 源（必应/百度/DuckDuckGo） |
| `vision_mode` | **识图能力**（设置里二选一）：`off`（默认，OCR 识别文字）/ `on`（把图片发给模型）。旧配置的 `auto` 会在加载时按模型能力注册表一次性解析为 `on`/`off` |
| `model_kind` | **出图能力**（设置里二选一）：`chat`（默认，不是出图模型，保持流式）/ `image`（是出图模型，走非流式；工具与联网搜索仍然可用）。与 `vision_mode` 相互独立，可同时开启。旧配置的 `auto`/`vision` 会在加载时按模型名一次性解析为 `image`/`chat` |
| `bg_color`、`bg_opacity`、`accent_color` | 背景色、透明度、强调色 |
| `msg_user_bg`、`msg_user_border` | 用户气泡配色微调 |
| `panel_width`、`panel_height` | 面板尺寸（拖拽后自动持久化） |
| `theme_mode` | `auto` / `dark` / `light` |
| `theme_preset` | `indigo` / `midnight` / `emerald` / `cloud` / `sakura` / `moss` / `custom` |
| `bubble_auto_collapse`、`bubble_collapse_delay` | 气泡自动收折开关与延迟（3 ~ 86400 秒） |

## 数据与隐私

所有数据保存在本地。

| 文件 / 目录 | 用途 |
|---|---|
| `config.json` | 应用配置（**含 AI API Key 与 Tavily Key 明文**，请注意保护） |
| `chat_history.db`（+ `-wal` / `-shm`） | SQLite：`conversations` / `messages` / `images` / `reminders` 四表，WAL 模式 |
| `images/` | 附件图片与 AI 生成图片（`{uuid}.{ext}`，不存 base64 入库） |
| `bootstrap.json` | 自定义数据目录注册表（仅迁移后存在） |

- 默认数据目录：`%APPDATA%\DesktopAide`
- 更换位置：**设置 → 通用 → 存储位置**（复制迁移，源数据保留，完成后自动重启）
- 备份/换机：直接拷贝整个数据目录即可

## 快捷键与斜杠命令

| 快捷键 | 作用 |
|---|---|
| `Enter` | 发送消息 |
| `Shift + Enter` | 换行 |
| `Ctrl + V` | 粘贴图片 / 文本文件为附件（输入框聚焦时） |
| `Ctrl + Alt + Space` | 全局：切换面板 |
| `Ctrl + Alt + C` | 全局：截图提问 |
| `Esc` | 收起面板 |

| 斜杠命令 | 作用 |
|---|---|
| `/clear` | 清空当前对话（可撤销） |
| `/copy` | 复制最后一条 AI 回复 |
| `/remind 10分钟 内容` | 快速创建定时提醒 |
| `/help` | 显示帮助 |

## 常见问题

**Q：联网搜索失败怎么办？**
按顺序排查：① 看日志（debug 构建）中 `[web_search]` 的 WARN，会写明是哪个源、什么错误；② 检查网络能否访问必应（免 Key 场景的主力源，DuckDuckGo 在部分网络不可达）；③ 如条件允许，在设置里填 Tavily API Key 提高稳定性。

**Q：截图识图 / 图片 OCR 不工作？**
确认 OCR 引擎在下列任一位置：`<安装目录>\resources\RapidOCR-json\RapidOCR-json.exe` 或 `%APPDATA%\DesktopAide\RapidOCR-json\`。免安装版只拷 exe、漏掉 `resources/` 目录时会出现此问题。

**Q：数据在哪？如何备份或换机？**
见 [数据与隐私](#数据与隐私)。直接拷贝数据目录即可；也可在设置里迁移到新位置。

## 已知限制

- **Anthropic 协议暂不支持工具调用**：`providers/anthropic.rs` 在携带 tools 时会明确报错（设计文档中列为 M2 阶段）。使用 Anthropic 服务商时请**关闭**「启用工具调用」与「允许 AI 联网搜索」，或改用 OpenAI 兼容服务商
- **单 Agent，无任务规划/子代理**，工具串行执行，无法自主推进长链路复杂任务
- **上下文只裁剪不摘要**：超过 20 轮 / 16000 token 后丢弃最旧轮次；会话重建后历史图片退化为 base64 文本（视觉模型无法再理解）
- **识图默认走 OCR**：`vision_mode` 默认 `off`，即使模型支持识图也不会自动发送图片，需要到「设置 → AI 服务 → 图片输入方式（识图能力）」切到「把图片发给模型」。这是刻意的保守默认，因为模型能力注册表会滞后也会误判（子串匹配会把 `moonshot-v1-8k` 匹配到 `moonshot-v1-8k-vision-preview`）
- **出图能力需要手动开启**：`model_kind` 默认 `chat`，不再按模型名自动判断，因此**使用出图模型时要记得把「模型用途」切到「是出图模型」**，否则请求会按流式对话发出、拿不到图片。反过来，模型名里带 `image` 但其实只做看图/对话的网关模型，保持默认即可，不会被误判
- **升级时的兼容处理**：旧配置里的 `model_kind = auto`/`vision` 与 `vision_mode = auto` 会在**配置加载时一次性解析**为具体取值（分别用同一套模型名启发式与能力注册表），因此升级不会静默改变已有行为；仅当模型名会命中启发式误判时，结果才与"手动选对"不同
- **免 Key 搜索源的局限**：百度的 HTML 结果页会返回「安全验证」，实际不可用；DuckDuckGo 在部分网络不可达。免 Key 场景实际依赖必应（RSS 端点）
- **release 构建无日志**；**未做代码签名**；**无自动更新**
- **仅支持 Windows**（依赖 WebView2、WinRT OCR、窗口置顶等平台能力）

## 许可证

本项目以 **MIT 许可证**发布，版权归 **DesktopAide contributors** 所有 —— 见 [LICENSE](LICENSE)。

简言之：你可以自由使用、修改、分发（含商业用途与闭源分发），只需**保留版权声明与许可证文本**；软件按「原样」提供，作者不承担担保与责任。

> **第三方组件**：本项目的安装包/免安装版**捆绑分发**了 RapidOCR-json 与 PP-OCR ONNX 模型（本地 OCR），并依赖大量开源库。`LICENSE` 与 `THIRD-PARTY-NOTICES.md`（列出各组件的许可证与来源）会一并放入安装目录。
