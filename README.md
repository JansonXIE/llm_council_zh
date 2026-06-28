# LLM Council

The idea of this repo is that instead of asking a question to your favorite LLM provider, you can group them into your "LLM Council". This repo is a simple, local web app that essentially looks like ChatGPT except it uses OpenRouter to send your query to multiple LLMs, it then asks them to review and rank each other's work, and finally a Chairman LLM produces the final response.

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

### 3. Configure Models

Models are now fully customizable from the in-app Settings panel. You no longer need to edit `.env` files or recompile to change the council.

Open the desktop app, click the **Settings** icon, and under **Council Models** click **+ 添加模型** to add each model you want in your council. For every model, fill in:

- **Model Name** — the model identifier sent to the API (e.g. `deepseek-chat`, `gpt-4o`, `claude-3-5-sonnet-20241022`)
- **API Key** — the secret key for that provider
- **Base URL** — the provider's API base URL (e.g. `https://api.deepseek.com`, `https://api.openai.com/v1`, `https://api.sophnet.com/v1`)
- **Request Format** — choose `OpenAI Compatible`, `Anthropic Messages`, or `Gemini generateContent`

URL handling depends on the selected request format:

- `OpenAI Compatible` appends `/chat/completions` if needed.
- `Anthropic Messages` appends `/messages` if needed. For SophNet, use `https://api.sophnet.com/v1`.
- `Gemini generateContent` uses a full `:generateContent` URL as-is. For SophNet Gemini, use `https://api.sophnet.com/v1beta/models/gemini-3.1-pro-preview:generateContent`.

Then pick one model as the **Chairman Model** from the dropdown — it synthesizes the final answer.

Toggle a model to **休眠** (dormant) to temporarily exclude it from council discussions without deleting it.

#### Optional: Environment Variable Overrides

For headless or pre-configured setups, you can still seed the council via environment variables in a `.env` file at the project root:

```bash
COUNCIL_MODELS=deepseek-chat,gpt-4o,claude-3-5-sonnet-20241022
CHAIRMAN_MODEL=deepseek-chat
```

Note: when using env vars, each model's API Key and Base URL must still be configured in the Settings panel, because models are now self-contained (each carries its own credentials).

## Running the Desktop App

```bash
cd frontend
npm run tauri dev
```

## Desktop Build (Native Executable)
```bash
npm run tauri:build
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
