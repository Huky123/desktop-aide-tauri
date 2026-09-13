# 第三方组件与许可声明（THIRD-PARTY NOTICES）

DesktopAide 本身以 **MIT 许可证**发布，版权归 *DesktopAide contributors*（见 [LICENSE](LICENSE)）。

本项目**捆绑分发**了若干第三方组件，并在构建/运行时依赖大量开源库。分发本项目的安装包或免安装版时，请连同本文件与 `LICENSE` 一起提供。

## 一、随安装包捆绑分发的组件（重点）

| 组件 | 位置 / 用途 | 许可证 | 上游 |
|---|---|---|---|
| **RapidOCR-json** | `resources/RapidOCR-json/RapidOCR-json.exe`，本地 OCR 推理可执行文件 | MIT（Copyright (c) 2023 hiroi-sora） | https://github.com/hiroi-sora/RapidOCR-json |
| **PP-OCR 系列 ONNX 模型** | `resources/RapidOCR-json/models/*.onnx` 及词表 `dict_*.txt`，文字检测 / 识别 | Apache-2.0（PaddleOCR 项目，具体以模型页声明为准） | https://github.com/PaddlePaddle/PaddleOCR |
| **WebView2 Runtime** | 界面渲染引擎（Windows 11 预装；Windows 10 由安装器引导安装） | Microsoft 专有许可（允许随应用分发） | https://developer.microsoft.com/microsoft-edge/webview2/ |
| **NSIS** | 生成 Windows 安装包所用工具链 | zlib/libpng 许可证 | https://nsis.sourceforge.io/ |

**RapidOCR-json 的传递依赖**：

| 组件 | 许可证 | 上游 |
|---|---|---|
| RapidOcrOnnx（推理引擎） | Apache-2.0 | https://github.com/RapidAI/RapidOcrOnnx |
| nlohmann/json（JSON 库） | MIT | https://github.com/nlohmann/json |


## 二、主要依赖（开源库）

### 前端（npm）

| 包 | 许可证 |
|---|---|
| react / react-dom | MIT |
| zustand | MIT |
| framer-motion | MIT |
| react-markdown / remark-gfm / remark-math / rehype-katex | MIT |
| katex | MIT |
| sonner | MIT |
| @tauri-apps/api、@tauri-apps/plugin-dialog | MIT 或 Apache-2.0 |
| vite / typescript / tailwindcss / vitest / eslint（开发依赖） | MIT / Apache-2.0 等 |

### 后端（crates.io）

| crate | 许可证 |
|---|---|
| tauri、tauri-plugin-* | MIT 或 Apache-2.0 |
| tokio、futures | MIT |
| reqwest | MIT 或 Apache-2.0 |
| serde、serde_json | MIT 或 Apache-2.0 |
| rusqlite（bundled SQLite） | MIT（SQLite 本身为 Public Domain） |
| chrono | MIT 或 Apache-2.0 |
| uuid | Apache-2.0 或 MIT |
| base64 | MIT 或 Apache-2.0 |
| dirs、once_cell、log | MIT 或 Apache-2.0 |
| eventsource-stream | MIT |
| windows（windows-rs） | MIT 或 Apache-2.0 |
| llm_models_spider | MIT（Copyright (c) 2024 Spider Cloud，已核对 crate 内 LICENSE） |
