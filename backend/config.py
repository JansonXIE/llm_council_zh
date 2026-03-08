"""Configuration for the LLM Council."""

import os
from dotenv import load_dotenv

load_dotenv()

# OpenRouter API key
OPENROUTER_API_KEY = os.getenv("OPENROUTER_API_KEY")

# Council members - list of OpenRouter model identifiers
COUNCIL_MODELS = [
    "deepseek/deepseek-chat",
    "minimax/MiniMax-M2.1",
    "kimi/kimi-k2-turbo-preview",
    "glm/glm-5",
]

# Chairman model - synthesizes final response
CHAIRMAN_MODEL = "deepseek/deepseek-chat"

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
