# LLM Council

![llmcouncil](header.jpg)

The idea of this repo is that instead of asking a question to your favorite LLM provider (e.g. OpenAI GPT 5.1, Google Gemini 3.0 Pro, Anthropic Claude Sonnet 4.5, xAI Grok 4, eg.c), you can group them into your "LLM Council". This repo is a simple, local web app that essentially looks like ChatGPT except it uses OpenRouter to send your query to multiple LLMs, it then asks them to review and rank each other's work, and finally a Chairman LLM produces the final response.

In a bit more detail, here is what happens when you submit a query:

1. **Stage 1: First opinions**. The user query is given to all LLMs individually, and the responses are collected. The individual responses are shown in a "tab view", so that the user can inspect them all one by one.
2. **Stage 2: Review**. Each individual LLM is given the responses of the other LLMs. Under the hood, the LLM identities are anonymized so that the LLM can't play favorites when judging their outputs. The LLM is asked to rank them in accuracy and insight.
3. **Stage 3: Final response**. The designated Chairman of the LLM Council takes all of the model's responses and compiles them into a single final answer that is presented to the user.

## Vibe Code Alert

This project was 99% vibe coded as a fun Saturday hack because I wanted to explore and evaluate a number of LLMs side by side in the process of [reading books together with LLMs](https://x.com/karpathy/status/1990577951671509438). It's nice and useful to see multiple responses side by side, and also the cross-opinions of all LLMs on each other's outputs. I'm not going to support it in any way, it's provided here as is for other people's inspiration and I don't intend to improve it. Code is ephemeral now and libraries are over, ask your LLM to change it in whatever way you like.

## Desktop Setup

The app now runs as a [Tauri](https://tauri.app/) desktop application. The React UI stays the same, but the old FastAPI backend has been replaced by a local Rust runtime inside the desktop app.

### 1. Install Prerequisites

- Install Node.js 20+
- Install Rust via [rustup](https://www.rust-lang.org/tools/install)
- On Windows, install **Microsoft C++ Build Tools** with "Desktop development with C++"
- Ensure **WebView2 Runtime** is installed

### 2. Install Frontend and Tauri Dependencies

```bash
cd frontend
npm install
```

### 3. Configure API Keys

Create a `.env` file in the project root. The Tauri runtime loads it automatically during development.

```bash
OPENROUTER_API_KEY=sk-or-v1-...
DEEPSEEK_API_KEY=...
DEEPSEEK_BASE_URL=https://api.deepseek.com
MINIMAX_API_KEY=...
MINIMAX_BASE_URL=https://api.minimax.chat/v1
KIMI_API_KEY=...
KIMI_BASE_URL=https://api.moonshot.cn/v1
GLM_API_KEY=...
GLM_BASE_URL=https://open.bigmodel.cn/api/paas/v4
```

Provider-prefixed models such as `deepseek/...` or `glm/...` will use their dedicated API if both key and base URL are configured. Otherwise the runtime falls back to OpenRouter.

### 4. Configure Models (Optional)

By default the desktop runtime uses the same council as [backend/config.py](backend/config.py). You can override it without recompiling by adding these optional environment variables:

```bash
COUNCIL_MODELS=deepseek/DeepSeek-V4-Pro,minimax/MiniMax-M3,kimi/Kimi-K2.6,glm/GLM-5.1
CHAIRMAN_MODEL=deepseek/DeepSeek-V4-Pro
```

## Running the Desktop App

```bash
cd frontend
npm run tauri dev
```

That starts Vite and the Tauri desktop shell together. You no longer need a separate Python backend process.

## Local Persistence

Conversations are stored as JSON files in Tauri's app data directory instead of [data/conversations](data/conversations).

- Windows: `%AppData%/com.jianxing.llm-council/conversations/`
- macOS: `~/Library/Application Support/com.jianxing.llm-council/conversations/`
- Linux: `~/.local/share/com.jianxing.llm-council/conversations/`

This keeps conversation data local to the desktop app and avoids exposing provider API keys to the browser runtime.

## Legacy Backend

The old Python backend under [backend](backend) is still in the repository as a reference implementation, but it is no longer required to run the app.

## Tech Stack

- **Desktop Runtime:** Tauri v2 + Rust + reqwest
- **Frontend:** React + Vite + react-markdown
- **Storage:** Local JSON files in the Tauri app data directory
- **Legacy Reference:** FastAPI backend kept in-repo but not used at runtime
