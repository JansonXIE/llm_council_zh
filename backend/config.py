"""Configuration for the LLM Council."""

import os
from dotenv import load_dotenv

load_dotenv()

# OpenRouter API key
OPENROUTER_API_KEY = os.getenv("OPENROUTER_API_KEY")

# Council members - legacy reference defaults
COUNCIL_MODELS = [
    "DeepSeek-V4-Pro",
    "MiniMax-M3",
    "Kimi-K2.6",
    "GLM-5.1",
]

# Chairman model - synthesizes final response
CHAIRMAN_MODEL = "DeepSeek-V4-Pro"

PROVIDERS = {
    "deepseek": {
        "api_key": os.getenv("DEEPSEEK_API_KEY"),
        "base_url": (os.getenv("DEEPSEEK_BASE_URL") or "").rstrip("/") + "/chat/completions",
    },
    "minimax": {
        "api_key": os.getenv("MINIMAX_API_KEY"),
        "base_url": (os.getenv("MINIMAX_BASE_URL") or "").rstrip("/") + "/chat/completions",
    },
    "kimi": {
        "api_key": os.getenv("KIMI_API_KEY"),
        "base_url": (os.getenv("KIMI_BASE_URL") or "").rstrip("/") + "/chat/completions",
    },
    "glm": {
        "api_key": os.getenv("GLM_API_KEY"),
        "base_url": (os.getenv("GLM_BASE_URL") or "").rstrip("/") + "/chat/completions",
    },
    "openrouter": {
        "api_key": os.getenv("OPENROUTER_API_KEY"),
        "base_url": "https://openrouter.ai/api/v1/chat/completions",
    }
}

# Data directory for conversation storage
DATA_DIR = "data/conversations"
