# LLM Council（模型议会）

与其只向单一的大语言模型（LLM）提问，不如把多个模型组成一个「LLM 议会」。本项目是一个本地桌面应用，界面类似 ChatGPT，但它会把你的问题同时发给多个 LLM，让它们互相评审、排名彼此的回答，最后由一位「主席模型（Chairman）」综合所有意见生成最终答案。

当你提交一个问题时，应用会经历以下三个阶段：

1. **阶段一：初步回答（First Opinions）**。问题被分别发送给所有议会模型，独立收集各自的回答。每个模型的回答以「标签页」形式展示，你可以逐一查看。
2. **阶段二：互评排名（Review）**。每个模型都会看到其他模型的回答。为避免模型偏袒，回答会被匿名化（仅以「回答 A、回答 B……」标识）。模型据此从准确性和洞察力两方面进行排名。
3. **阶段三：最终答案（Final Response）**。指定的主席模型综合所有模型的回答与排名意见，整合成一份最终答案呈现给你。

> 本项目是 [Andrej Karpathy 的 LLM Council](https://github.com/karpathy/llm-council) 的中文化与桌面化改造版本，并针对国内常见的模型服务（如 DeepSeek、SophNet 等）增加了多种 API 请求格式支持。

---

## 主要功能

- **多模型并行讨论**：同时调用多个模型，三阶段「回答 → 互评 → 综合」流程。
- **可一键关闭委员会**：在设置中关闭「启用大模型委员会」后，将跳过阶段二、阶段三，直接由主席模型（Chairman）回答问题，更快更省。
- **完全可配置**：所有模型均在应用内「设置」面板中管理，无需修改代码或重新编译。
- **多种 API 请求格式**：每个模型可独立选择请求格式，方便混用不同厂商：
  - `OpenAI Compatible`（OpenAI 兼容）
  - `Anthropic Messages`（Anthropic Messages 接口）
  - `Gemini generateContent`（Gemini generateContent 接口）
- **匿名互评**：阶段二对模型身份匿名化，避免互相偏袒；前端会反匿名化展示，方便你核对。
- **主席模型可自选**：从已配置的模型中任意指定一个作为主席。
- **模型休眠**：可将某个模型临时设为「休眠」，暂时不参与讨论但保留配置。
- **本地持久化**：对话记录以 JSON 文件保存在本地，可自定义保存目录。
- **自动更新检查**：可在设置中开启「启动时自动检查更新」。
- **纯本地运行**：基于 Tauri 桌面运行时，API Key 仅保存在本地，不暴露给浏览器环境。

---

## 环境准备

本应用基于 [Tauri](https://tauri.app/) 桌面框架，使用 React 前端 + 本地 Rust 运行时（已替代旧版 FastAPI 后端）。

请先安装以下依赖：

- **Node.js 20+**
- **Rust**（通过 [rustup](https://www.rust-lang.org/tools/install) 安装）
- **Windows 用户**：安装 **Microsoft C++ 生成工具（Build Tools）**，勾选「使用 C++ 的桌面开发」工作负载
- 确保已安装 **WebView2 运行时**（Windows 10/11 通常已自带）

---

## 安装与运行

### 1. 安装前端与 Tauri 依赖

```bash
cd frontend
npm install
```

### 2. 启动桌面应用（开发模式）

```bash
cd frontend
npm run tauri dev
```

该命令会同时启动 Vite 前端与 Tauri 桌面外壳，无需单独运行 Python 后端。

### 3. 打包为原生可执行程序

```bash
cd frontend
npm run tauri:build
```

打包产物位于 `frontend/src-tauri/target/release/`（安装包位于其 `bundle/` 子目录）。

---

## 配置模型

所有模型都在应用内「设置」面板中管理，**无需编辑配置文件或重新编译**。

打开应用后，点击 **设置（⚙）** 图标，在 **Council Models** 区域点击 **+ 添加模型**，为每个模型填写：

| 字段 | 说明 | 示例 |
| --- | --- | --- |
| **模型名称** | 调用 API 时使用的模型标识符 | `deepseek-chat`、`gpt-4o`、`claude-3-5-sonnet-20241022` |
| **API Key** | 对应厂商的密钥 | `sk-xxxxxxxx` |
| **Base URL** | 厂商的 API 基础地址 | `https://api.deepseek.com`、`https://api.openai.com/v1`、`https://api.sophnet.com/v1` |
| **请求格式** | 选择该模型使用的接口协议 | `OpenAI Compatible` / `Anthropic Messages` / `Gemini generateContent` |

### Base URL 的处理规则

URL 的补全方式取决于所选的**请求格式**：

- **`OpenAI Compatible`**：若地址未包含路径，会自动补全 `/chat/completions`。
- **`Anthropic Messages`**：会自动补全 `/messages`。SophNet 可直接填写 `https://api.sophnet.com/v1`。
- **`Gemini generateContent`**：使用以 `:generateContent` 结尾的完整地址，**原样发送**。例如 SophNet 的 Gemini 接口可填写：
  `https://api.sophnet.com/v1beta/models/gemini-3.1-pro-preview:generateContent`

### 选择主席模型

在 **Chairman Model** 下拉框中选择一个模型作为主席，由它综合生成最终答案。下拉框会列出所有已配置的模型；休眠的模型会标注「（休眠）」，失效（已删除/改名）的旧主席会标注「（已失效，请重新选择）」以便你及时更正。

### 启用 / 关闭大模型委员会

在 **Council Models** 区域顶部有「启用大模型委员会」开关：

- **开启（默认）**：执行完整的三阶段流程（阶段一回答 → 阶段二互评 → 阶段三综合）。
- **关闭**：跳过阶段二与阶段三，直接由所选的**主席模型（Chairman）**回答问题，响应更快、成本更低。此时界面只展示主席模型的最终回答。

### 模型休眠

点击模型卡片上的开关可将其切换为 **休眠** 状态，使其暂时不参与议会讨论，但保留配置，无需删除。

### 数据保存路径

在「数据存储」区域可自定义对话记录的保存目录，留空则使用应用默认目录（见下文「本地持久化」）。

### 自动更新

在「关于与更新」区域可查看当前版本，并开关「启动时自动检查更新」。

---

## 可选：环境变量预置

对于无界面或预配置场景，仍可通过项目根目录下的 `.env` 文件预置议会成员：

```bash
COUNCIL_MODELS=deepseek-chat,gpt-4o,claude-3-5-sonnet-20241022
CHAIRMAN_MODEL=deepseek-chat
```

> 注意：使用环境变量时，每个模型的 **API Key** 和 **Base URL** 仍需在「设置」面板中配置，因为每个模型都是自包含的（各自携带凭据）。此外，**设置面板中配置的主席优先于环境变量**。

---

## 本地持久化

对话以 JSON 文件形式保存在 Tauri 的应用数据目录中（可在设置中自定义）。默认路径为：

- **Windows**：`%AppData%/com.jianxing.llm-council/conversations/`
- **macOS**：`~/Library/Application Support/com.jianxing.llm-council/conversations/`
- **Linux**：`~/.local/share/com.jianxing.llm-council/conversations/`

这样可将对话数据保留在本地，并避免把厂商 API Key 暴露给浏览器运行时。

---

## 技术栈

- **桌面运行时**：Tauri v2 + Rust + reqwest
- **前端**：React + Vite + react-markdown
- **存储**：Tauri 应用数据目录中的本地 JSON 文件
- **遗留参考**：仓库内保留的旧版 FastAPI 后端（`backend/`），仅作参考，运行时不再需要

---

## 遗留后端说明

仓库中 `backend/` 目录下的旧版 Python 后端作为参考实现保留，但运行本应用时**不再需要**它。

---

## 致谢

灵感来自 Andrej Karpathy 的 [llm-council](https://github.com/karpathy/llm-council) 与他关于 [与 LLM 一起读书](https://x.com/karpathy/status/1990577951671509438) 的想法。本项目在其基础上进行了中文化、桌面化与多 API 格式适配，仅供学习与交流使用。
