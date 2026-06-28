use chrono::Utc;
use futures::future::join_all;
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State, WebviewWindow};
use uuid::Uuid;

const STREAM_EVENT_NAME: &str = "council-event";

// ── Settings ──────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Default)]
struct ModelEntry {
    name: String,
    #[serde(default)]
    api_key: String,
    #[serde(default)]
    base_url: String,
    #[serde(default = "default_api_format")]
    api_format: String,
    #[serde(default = "default_true")]
    active: bool,
}

fn default_api_format() -> String {
    "openai".to_string()
}

fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Default)]
struct AppSettings {
    data_dir: String,
    #[serde(default)]
    models: Vec<ModelEntry>,
    #[serde(default)]
    chairman_model: String,
    #[serde(default)]
    image_model: String,
    #[serde(default = "default_true")]
    auto_update: bool,
    #[serde(default = "default_true")]
    council_enabled: bool,
}

// ── Core types ────────────────────────────────────────────────────────

#[derive(Clone)]
struct AppState {
    client: Client,
    config: Arc<RwLock<CouncilConfig>>,
}

#[derive(Clone)]
struct CouncilConfig {
    chairman_model: String,
    council_models: Vec<String>,
    model_registry: HashMap<String, ModelEntry>,
    data_dir: Option<String>,
    council_enabled: bool,
    image_model: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct Conversation {
    id: String,
    created_at: String,
    title: String,
    messages: Vec<ConversationMessage>,
}

#[derive(Serialize, Deserialize, Clone)]
struct ConversationMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    images: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stage1: Option<Vec<Stage1Response>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stage2: Option<Vec<Stage2Response>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stage3: Option<Stage3Response>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<CouncilMetadata>,
}

#[derive(Serialize, Deserialize, Clone)]
struct ConversationMetadata {
    id: String,
    created_at: String,
    title: String,
    message_count: usize,
}

#[derive(Serialize, Deserialize, Clone)]
struct Stage1Response {
    model: String,
    response: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct Stage2Response {
    model: String,
    ranking: String,
    parsed_ranking: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone)]
struct Stage3Response {
    model: String,
    response: String,
}

#[derive(Serialize, Deserialize, Clone)]
struct AggregateRanking {
    model: String,
    average_rank: f64,
    rankings_count: usize,
}

#[derive(Serialize, Deserialize, Clone, Default)]
struct CouncilMetadata {
    #[serde(default)]
    label_to_model: HashMap<String, String>,
    #[serde(default)]
    aggregate_rankings: Vec<AggregateRanking>,
}

#[derive(Serialize, Deserialize, Clone)]
struct SendMessageResponse {
    stage1: Vec<Stage1Response>,
    stage2: Vec<Stage2Response>,
    stage3: Stage3Response,
    metadata: CouncilMetadata,
}

#[derive(Serialize, Clone)]
struct StreamEvent {
    #[serde(rename = "type")]
    event_type: String,
    conversation_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<CouncilMetadata>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[derive(Serialize, Clone)]
struct ChatMessage {
    role: String,
    content: String,
    /// Base64 data URLs (e.g. "data:image/png;base64,....") attached to this
    /// message. Empty for text-only messages. Used for multimodal requests.
    #[serde(skip)]
    images: Vec<String>,
}

impl ChatMessage {
    fn text(role: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            role: role.into(),
            content: content.into(),
            images: Vec::new(),
        }
    }
}

struct ModelReply {
    content: String,
}

// ── Settings persistence ──────────────────────────────────────────────

fn settings_path(app_handle: &AppHandle) -> Result<PathBuf, String> {
    let mut path = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    path.push("settings.json");
    Ok(path)
}

fn load_settings_from_file(app_handle: &AppHandle) -> AppSettings {
    let path = match settings_path(app_handle) {
        Ok(p) => p,
        Err(_) => return AppSettings::default(),
    };

    if !path.exists() {
        return AppSettings::default();
    }

    let contents = std::fs::read_to_string(&path).unwrap_or_default();
    serde_json::from_str::<AppSettings>(&contents).unwrap_or_default()
}

fn save_settings_to_file(app_handle: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    let path = settings_path(app_handle)?;

    // Ensure directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}

// ── Build config ──────────────────────────────────────────────────────

fn env_var(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn normalize_chat_completions_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.ends_with("/chat/completions") {
        Some(trimmed.to_string())
    } else {
        Some(format!("{trimmed}/chat/completions"))
    }
}

fn normalize_anthropic_messages_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.ends_with("/messages") {
        Some(trimmed.to_string())
    } else {
        Some(format!("{trimmed}/messages"))
    }
}

fn normalize_gemini_generate_content_url(raw: &str, model: &str) -> Option<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains(":generateContent") {
        Some(trimmed.to_string())
    } else {
        Some(format!("{trimmed}/models/{model}:generateContent"))
    }
}

fn build_council_config(settings: &AppSettings) -> CouncilConfig {
    dotenvy::dotenv().ok();

    // Build model registry from settings.models
    let mut model_registry: HashMap<String, ModelEntry> = HashMap::new();
    for entry in &settings.models {
        let trimmed_name = entry.name.trim().to_string();
        if trimmed_name.is_empty() {
            continue;
        }
        model_registry.insert(trimmed_name.clone(), entry.clone());
    }

    // Council models: active entries from registry
    let council_models = env_var("COUNCIL_MODELS")
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|models| !models.is_empty())
        .unwrap_or_else(|| {
            settings
                .models
                .iter()
                .filter(|entry| entry.active && !entry.name.trim().is_empty())
                .map(|entry| entry.name.trim().to_string())
                .collect()
        });

    let chairman_model = if !settings.chairman_model.trim().is_empty() {
        settings.chairman_model.trim().to_string()
    } else {
        env_var("CHAIRMAN_MODEL").unwrap_or_default()
    };

    // Data directory: prefer settings, fall back to default
    let data_dir = if !settings.data_dir.is_empty() {
        Some(settings.data_dir.clone())
    } else {
        None
    };

    let image_model = if !settings.image_model.trim().is_empty() {
        settings.image_model.trim().to_string()
    } else {
        env_var("IMAGE_MODEL").unwrap_or_default()
    };

    // The dedicated image-analysis model should not take part in the council's
    // multi-stage deliberation (stage 1 responses / stage 2 peer review), so we
    // drop it from the council roster here.
    let council_models = if image_model.trim().is_empty() {
        council_models
    } else {
        council_models
            .into_iter()
            .filter(|name| name.trim() != image_model.trim())
            .collect()
    };

    CouncilConfig {
        chairman_model,
        council_models,
        model_registry,
        data_dir,
        council_enabled: settings.council_enabled,
        image_model,
    }
}

// ── AppState impl ─────────────────────────────────────────────────────

impl AppState {
    fn new(app_handle: &AppHandle) -> Self {
        let settings = load_settings_from_file(app_handle);
        let config = build_council_config(&settings);

        Self {
            client: Client::new(),
            config: Arc::new(RwLock::new(config)),
        }
    }
}

// ── StreamEvent impl ──────────────────────────────────────────────────

impl StreamEvent {
    fn new(conversation_id: &str, event_type: &str) -> Self {
        Self {
            event_type: event_type.to_string(),
            conversation_id: conversation_id.to_string(),
            data: None,
            metadata: None,
            message: None,
        }
    }

    fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }

    fn with_metadata(mut self, metadata: CouncilMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}

// ── Helper functions ──────────────────────────────────────────────────

fn round_to_2(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

fn assistant_message_from_response(response: &SendMessageResponse) -> ConversationMessage {
    ConversationMessage {
        role: "assistant".to_string(),
        content: None,
        images: Vec::new(),
        stage1: Some(response.stage1.clone()),
        stage2: Some(response.stage2.clone()),
        stage3: Some(response.stage3.clone()),
        metadata: Some(response.metadata.clone()),
    }
}

fn user_message(content: String, images: Vec<String>) -> ConversationMessage {
    ConversationMessage {
        role: "user".to_string(),
        content: Some(content),
        images,
        stage1: None,
        stage2: None,
        stage3: None,
        metadata: None,
    }
}

fn extract_content(data: &Value) -> String {
    let content = &data["choices"][0]["message"]["content"];

    if let Some(text) = content.as_str() {
        return text.to_string();
    }

    content
        .as_array()
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn extract_anthropic_content(data: &Value) -> String {
    data["content"]
        .as_array()
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part.get("text").and_then(Value::as_str))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn extract_gemini_content(data: &Value) -> String {
    data["candidates"]
        .as_array()
        .map(|candidates| {
            candidates
                .iter()
                .flat_map(|candidate| {
                    candidate["content"]["parts"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default()
                })
                .filter_map(|part| part.get("text").and_then(Value::as_str).map(ToOwned::to_owned))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn split_system_messages(messages: &[ChatMessage]) -> (String, Vec<ChatMessage>) {
    let system = messages
        .iter()
        .filter(|message| message.role == "system")
        .map(|message| message.content.clone())
        .collect::<Vec<_>>()
        .join("\n\n");

    let conversation = messages
        .iter()
        .filter(|message| message.role != "system")
        .cloned()
        .collect::<Vec<_>>();

    (system, conversation)
}

/// Split a base64 data URL ("data:image/png;base64,XXXX") into its mime type and
/// raw base64 payload. Falls back to "image/png" when the mime cannot be parsed.
fn parse_data_url(data_url: &str) -> (String, String) {
    if let Some(rest) = data_url.strip_prefix("data:") {
        if let Some((meta, payload)) = rest.split_once(',') {
            let mime = meta.split(';').next().unwrap_or("image/png").to_string();
            return (mime, payload.to_string());
        }
    }
    // Treat the whole string as raw base64 with a default mime.
    ("image/png".to_string(), data_url.to_string())
}

/// Build the OpenAI-compatible `content` field for a single message. When the
/// message carries images we emit the multimodal array form; otherwise a plain
/// string is used to stay compatible with text-only providers.
fn openai_message_content(message: &ChatMessage) -> Value {
    if message.images.is_empty() {
        return json!(message.content);
    }

    let mut parts = Vec::new();
    if !message.content.is_empty() {
        parts.push(json!({ "type": "text", "text": message.content }));
    }
    for image in &message.images {
        parts.push(json!({
            "type": "image_url",
            "image_url": { "url": image },
        }));
    }
    json!(parts)
}

fn openai_messages_payload(messages: &[ChatMessage]) -> Value {
    let array = messages
        .iter()
        .map(|message| {
            json!({
                "role": message.role,
                "content": openai_message_content(message),
            })
        })
        .collect::<Vec<_>>();
    json!(array)
}

fn anthropic_payload(model: &str, messages: &[ChatMessage]) -> Value {
    let (system, conversation) = split_system_messages(messages);
    let messages_json = conversation
        .iter()
        .map(|message| {
            if message.images.is_empty() {
                json!({
                    "role": message.role,
                    "content": message.content,
                })
            } else {
                let mut parts = Vec::new();
                for image in &message.images {
                    let (mime, data) = parse_data_url(image);
                    parts.push(json!({
                        "type": "image",
                        "source": {
                            "type": "base64",
                            "media_type": mime,
                            "data": data,
                        },
                    }));
                }
                if !message.content.is_empty() {
                    parts.push(json!({ "type": "text", "text": message.content }));
                }
                json!({
                    "role": message.role,
                    "content": parts,
                })
            }
        })
        .collect::<Vec<_>>();

    let mut payload = json!({
        "model": model,
        "max_tokens": 4096,
        "messages": messages_json,
    });
    if !system.is_empty() {
        payload["system"] = json!(system);
    }
    payload
}

fn gemini_payload(messages: &[ChatMessage]) -> Value {
    let (system, conversation) = split_system_messages(messages);
    let contents = conversation
        .iter()
        .map(|message| {
            let mut parts = Vec::new();
            if !message.content.is_empty() {
                parts.push(json!({ "text": message.content }));
            }
            for image in &message.images {
                let (mime, data) = parse_data_url(image);
                parts.push(json!({
                    "inline_data": {
                        "mime_type": mime,
                        "data": data,
                    },
                }));
            }
            json!({
                "role": if message.role == "assistant" { "model" } else { "user" },
                "parts": parts,
            })
        })
        .collect::<Vec<_>>();

    let mut payload = json!({
        "contents": contents,
        "generationConfig": {
            "maxOutputTokens": 4096,
        },
    });
    if !system.is_empty() {
        payload["systemInstruction"] = json!({
            "parts": [{ "text": system }],
        });
    }
    payload
}

fn serialize_value<T: Serialize>(value: &T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|error| error.to_string())
}

fn extract_conversation_history(conversation: &Conversation, max_turns: usize) -> Vec<ChatMessage> {
    let mut history = Vec::new();
    let mut turns = 0;
    for msg in conversation.messages.iter().rev() {
        if msg.role == "assistant" {
            if let Some(stage3) = &msg.stage3 {
                history.push(ChatMessage::text("assistant", stage3.response.clone()));
            }
        } else if msg.role == "user" {
            if let Some(content) = &msg.content {
                history.push(ChatMessage {
                    role: "user".to_string(),
                    content: content.clone(),
                    images: msg.images.clone(),
                });
                turns += 1;
                if turns >= max_turns {
                    break;
                }
            }
        }
    }
    history.reverse();
    history
}

// ── Query model ───────────────────────────────────────────────────────

async fn query_model(
    client: &Client,
    config: &Arc<RwLock<CouncilConfig>>,
    model: &str,
    messages: &[ChatMessage],
    timeout_secs: f64,
) -> Option<ModelReply> {
    // Extract needed data from config and release lock immediately
    let (api_key, api_url, request_model, api_format) = {
        let cfg = config.read().unwrap();

        // Look up the model entry directly in the registry
        let entry = cfg.model_registry.get(model)?;

        let key = if entry.api_key.trim().is_empty() {
            return None;
        } else {
            entry.api_key.trim().to_string()
        };

        let format = if entry.api_format.trim().is_empty() {
            default_api_format()
        } else {
            entry.api_format.trim().to_string()
        };
        let request_model = entry.name.trim().to_string();
        let url = match format.as_str() {
            "anthropic_messages" => normalize_anthropic_messages_url(&entry.base_url)?,
            "gemini_messages" => normalize_gemini_generate_content_url(&entry.base_url, &request_model)?,
            _ => normalize_chat_completions_url(&entry.base_url)?,
        };

        // The request model id sent to the API is exactly the model name entered by the user.
        (key, url, request_model, format)
    }; // cfg dropped here, before any .await

    let payload = match api_format.as_str() {
        "anthropic_messages" => anthropic_payload(&request_model, messages),
        "gemini_messages" => gemini_payload(messages),
        _ => json!({
            "model": request_model,
            "messages": openai_messages_payload(messages),
        }),
    };

    let mut request = client
        .post(api_url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .timeout(Duration::from_secs_f64(timeout_secs));

    if api_format == "anthropic_messages" {
        request = request
            .header("Accept", "application/json")
            .header("x-api-key", api_key.clone())
            .header("anthropic-version", "2023-06-01");
    } else if api_format == "gemini_messages" {
        request = request.header("x-api-key", api_key.clone());
    }

    let response = request.json(&payload).send().await;

    let response = match response {
        Ok(response) => response,
        Err(error) => {
            eprintln!("Error querying model {model}: {error}");
            return None;
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        eprintln!("Error querying model {model}: {status} {body}");
        return None;
    }

    let data = match response.json::<Value>().await {
        Ok(data) => data,
        Err(error) => {
            eprintln!("Error parsing response for model {model}: {error}");
            return None;
        }
    };

    let content = match api_format.as_str() {
        "anthropic_messages" => extract_anthropic_content(&data),
        "gemini_messages" => extract_gemini_content(&data),
        _ => extract_content(&data),
    };

    Some(ModelReply { content })
}

async fn query_models_parallel(
    client: &Client,
    config: Arc<RwLock<CouncilConfig>>,
    models: &[String],
    messages: Vec<ChatMessage>,
) -> Vec<(String, Option<ModelReply>)> {
    let tasks = models
        .iter()
        .cloned()
        .map(|model| {
            let client = client.clone();
            let config = config.clone();
            let messages = messages.clone();

            async move {
                let reply = query_model(&client, &config, &model, &messages, 120.0).await;
                (model, reply)
            }
        })
        .collect::<Vec<_>>();

    join_all(tasks).await
}

// ── Council stages ────────────────────────────────────────────────────

async fn stage1_collect_responses(
    client: &Client,
    config: Arc<RwLock<CouncilConfig>>,
    history: &[ChatMessage],
    user_query: &str,
) -> Vec<Stage1Response> {
    let mut messages = history.to_vec();
    messages.push(ChatMessage::text("user", user_query.to_string()));

    let models = {
        let cfg = config.read().unwrap();
        cfg.council_models.clone()
    }; // cfg dropped

    query_models_parallel(client, config.clone(), &models, messages)
        .await
        .into_iter()
        .filter_map(|(model, response)| {
            response.map(|reply| Stage1Response {
                model,
                response: reply.content,
            })
        })
        .collect()
}

async fn stage2_collect_rankings(
    client: &Client,
    config: Arc<RwLock<CouncilConfig>>,
    history: &[ChatMessage],
    user_query: &str,
    stage1_results: &[Stage1Response],
) -> (Vec<Stage2Response>, HashMap<String, String>) {
    let labels = (0..stage1_results.len())
        .map(|index| format!("Response {}", (b'A' + index as u8) as char))
        .collect::<Vec<_>>();

    let label_to_model = labels
        .iter()
        .zip(stage1_results.iter())
        .map(|(label, result)| (label.clone(), result.model.clone()))
        .collect::<HashMap<_, _>>();

    let responses_text = labels
        .iter()
        .zip(stage1_results.iter())
        .map(|(label, result)| format!("{label}:\n{}", result.response))
        .collect::<Vec<_>>()
        .join("\n\n");

    let ranking_prompt = format!(
        "Please reply in Chinese. You are evaluating different responses to the following question:\n\nQuestion: {user_query}\n\nHere are the responses from different models (anonymized):\n\n{responses_text}\n\nYour task:\n1. First, evaluate each response individually. For each response, explain what it does well and what it does poorly.\n2. Then, at the very end of your response, provide a final ranking.\n\nIMPORTANT: Your final ranking MUST be formatted EXACTLY as follows:\n- Start with the line \"FINAL RANKING:\" (all caps, with colon)\n- Then list the responses from best to worst as a numbered list\n- Each line should be: number, period, space, then ONLY the response label (e.g., \"1. Response A\")\n- Do not add any other text or explanations in the ranking section\n\nExample of the correct format for your ENTIRE response:\n\nResponse A provides good detail on X but misses Y...\nResponse B is accurate but lacks depth on Z...\nResponse C offers the most comprehensive answer...\n\nFINAL RANKING:\n1. Response C\n2. Response A\n3. Response B\n\nNow provide your evaluation and ranking:"
    );

    let mut messages = history.to_vec();
    messages.push(ChatMessage::text("user", ranking_prompt));

    let models = config.read().unwrap().council_models.clone();

    let stage2_results = query_models_parallel(client, config.clone(), &models, messages)
        .await
        .into_iter()
        .filter_map(|(model, response)| {
            response.map(|reply| Stage2Response {
                model,
                parsed_ranking: parse_ranking_from_text(&reply.content),
                ranking: reply.content,
            })
        })
        .collect::<Vec<_>>();

    (stage2_results, label_to_model)
}

async fn stage3_synthesize_final(
    client: &Client,
    config: Arc<RwLock<CouncilConfig>>,
    history: &[ChatMessage],
    user_query: &str,
    stage1_results: &[Stage1Response],
    stage2_results: &[Stage2Response],
) -> Stage3Response {
    let stage1_text = stage1_results
        .iter()
        .map(|result| format!("Model: {}\nResponse: {}", result.model, result.response))
        .collect::<Vec<_>>()
        .join("\n\n");

    let stage2_text = stage2_results
        .iter()
        .map(|result| format!("Model: {}\nRanking: {}", result.model, result.ranking))
        .collect::<Vec<_>>()
        .join("\n\n");

    let chairman_prompt = format!(
        "Please reply in Chinese. You are the Chairman of an LLM Council. Multiple AI models have provided responses to a user's question, and then ranked each other's responses.\n\nOriginal Question: {user_query}\n\nSTAGE 1 - Individual Responses:\n{stage1_text}\n\nSTAGE 2 - Peer Rankings:\n{stage2_text}\n\nYour task as Chairman is to synthesize all of this information into a single, comprehensive, accurate answer to the user's original question. Consider:\n- The individual responses and their insights\n- The peer rankings and what they reveal about response quality\n- Any patterns of agreement or disagreement\n\nProvide a clear, well-reasoned final answer that represents the council's collective wisdom:"
    );

    let mut messages = history.to_vec();
    messages.push(ChatMessage::text("user", chairman_prompt));

    let (chairman_model, chairman_known, chairman_has_key) = {
        let cfg = config.read().unwrap();
        let name = cfg.chairman_model.clone();
        let entry = cfg.model_registry.get(name.trim());
        let known = entry.is_some();
        let has_key = entry
            .map(|entry| !entry.api_key.trim().is_empty())
            .unwrap_or(false);
        (name, known, has_key)
    }; // cfg dropped

    // Surface configuration problems explicitly instead of a generic failure.
    if chairman_model.trim().is_empty() {
        return Stage3Response {
            model: "error".to_string(),
            response: "Error: No Chairman model is configured. Please pick a Chairman in Settings."
                .to_string(),
        };
    }

    if !chairman_known {
        return Stage3Response {
            model: chairman_model.clone(),
            response: format!(
                "Error: Chairman model \"{chairman_model}\" was not found in your configured models. Make sure the model exists in Settings and the name matches exactly."
            ),
        };
    }

    if !chairman_has_key {
        return Stage3Response {
            model: chairman_model.clone(),
            response: format!(
                "Error: Chairman model \"{chairman_model}\" has no API Key configured. Please add its API Key in Settings."
            ),
        };
    }

    let response = query_model(client, &config, &chairman_model, &messages, 120.0).await;

    Stage3Response {
        model: chairman_model,
        response: response
            .map(|reply| reply.content)
            .unwrap_or_else(|| {
                "Error: Chairman request failed (network error, timeout, or invalid response). Check the Base URL and request format in Settings.".to_string()
            }),
    }
}

/// Analyze attached images and answer the question directly using the
/// dedicated image model configured in Settings. Bypasses the council entirely
/// and returns a Stage3Response so the answer renders in the Final Response slot.
async fn image_analysis_response(
    client: &Client,
    config: Arc<RwLock<CouncilConfig>>,
    history: &[ChatMessage],
    user_query: &str,
    images: &[String],
) -> Stage3Response {
    let (image_model, model_known, model_has_key) = {
        let cfg = config.read().unwrap();
        let name = cfg.image_model.clone();
        let entry = cfg.model_registry.get(name.trim());
        let known = entry.is_some();
        let has_key = entry
            .map(|entry| !entry.api_key.trim().is_empty())
            .unwrap_or(false);
        (name, known, has_key)
    }; // cfg dropped

    if image_model.trim().is_empty() {
        return Stage3Response {
            model: "error".to_string(),
            response: "错误：尚未配置图片分析模型，请在「设置」中选择一个用于分析图片的模型。"
                .to_string(),
        };
    }

    if !model_known {
        return Stage3Response {
            model: image_model.clone(),
            response: format!(
                "错误：图片分析模型「{image_model}」未在已配置的模型列表中找到，请在「设置」中确认模型名称完全一致。"
            ),
        };
    }

    if !model_has_key {
        return Stage3Response {
            model: image_model.clone(),
            response: format!(
                "错误：图片分析模型「{image_model}」未配置 API Key，请在「设置」中补充。"
            ),
        };
    }

    let query_text = if user_query.trim().is_empty() {
        "请用中文详细描述并分析这张图片的内容。".to_string()
    } else {
        format!("请用中文回答。\n\n{user_query}")
    };

    let mut messages = history.to_vec();
    messages.push(ChatMessage {
        role: "user".to_string(),
        content: query_text,
        images: images.to_vec(),
    });

    let response = query_model(client, &config, &image_model, &messages, 180.0).await;

    Stage3Response {
        model: image_model,
        response: response.map(|reply| reply.content).unwrap_or_else(|| {
            "错误：图片分析请求失败（网络错误、超时或返回无效）。请检查该模型的 Base URL、请求格式，并确认其支持图片输入。".to_string()
        }),
    }
}

/// When the council feature is disabled, answer the question directly with the
/// Chairman model and skip the peer-review (stage 2) and synthesis (stage 3)
/// orchestration. Returns a Stage3Response so the UI can render the answer in
/// the familiar Final Response slot.
async fn chairman_only_response(
    client: &Client,
    config: Arc<RwLock<CouncilConfig>>,
    history: &[ChatMessage],
    user_query: &str,
) -> Stage3Response {
    let (chairman_model, chairman_known, chairman_has_key) = {
        let cfg = config.read().unwrap();
        let name = cfg.chairman_model.clone();
        let entry = cfg.model_registry.get(name.trim());
        let known = entry.is_some();
        let has_key = entry
            .map(|entry| !entry.api_key.trim().is_empty())
            .unwrap_or(false);
        (name, known, has_key)
    }; // cfg dropped

    if chairman_model.trim().is_empty() {
        return Stage3Response {
            model: "error".to_string(),
            response: "Error: No Chairman model is configured. Please pick a Chairman in Settings."
                .to_string(),
        };
    }

    if !chairman_known {
        return Stage3Response {
            model: chairman_model.clone(),
            response: format!(
                "Error: Chairman model \"{chairman_model}\" was not found in your configured models. Make sure the model exists in Settings and the name matches exactly."
            ),
        };
    }

    if !chairman_has_key {
        return Stage3Response {
            model: chairman_model.clone(),
            response: format!(
                "Error: Chairman model \"{chairman_model}\" has no API Key configured. Please add its API Key in Settings."
            ),
        };
    }

    let mut messages = history.to_vec();
    messages.push(ChatMessage::text(
        "user",
        format!("Please reply in Chinese.\n\n{user_query}"),
    ));

    let response = query_model(client, &config, &chairman_model, &messages, 120.0).await;

    Stage3Response {
        model: chairman_model,
        response: response
            .map(|reply| reply.content)
            .unwrap_or_else(|| {
                "Error: Chairman request failed (network error, timeout, or invalid response). Check the Base URL and request format in Settings.".to_string()
            }),
    }
}

fn parse_ranking_from_text(ranking_text: &str) -> Vec<String> {
    let pattern = Regex::new(r"Response [A-Z]").expect("valid ranking regex");

    if let Some((_, ranking_section)) = ranking_text.split_once("FINAL RANKING:") {
        let numbered = Regex::new(r"\d+\.\s*Response [A-Z]").expect("valid numbered ranking regex");
        let numbered_matches = numbered.find_iter(ranking_section).collect::<Vec<_>>();
        if !numbered_matches.is_empty() {
            return numbered_matches
                .into_iter()
                .filter_map(|capture| {
                    pattern
                        .find(capture.as_str())
                        .map(|value| value.as_str().to_string())
                })
                .collect();
        }

        return pattern
            .find_iter(ranking_section)
            .map(|value| value.as_str().to_string())
            .collect();
    }

    pattern
        .find_iter(ranking_text)
        .map(|value| value.as_str().to_string())
        .collect()
}

fn calculate_aggregate_rankings(
    stage2_results: &[Stage2Response],
    label_to_model: &HashMap<String, String>,
) -> Vec<AggregateRanking> {
    let mut positions: HashMap<String, Vec<usize>> = HashMap::new();

    for ranking in stage2_results {
        for (index, label) in parse_ranking_from_text(&ranking.ranking).iter().enumerate() {
            if let Some(model_name) = label_to_model.get(label) {
                positions
                    .entry(model_name.clone())
                    .or_default()
                    .push(index + 1);
            }
        }
    }

    let mut aggregate = positions
        .into_iter()
        .filter_map(|(model, values)| {
            if values.is_empty() {
                return None;
            }

            let average_rank = values.iter().sum::<usize>() as f64 / values.len() as f64;
            Some(AggregateRanking {
                model,
                average_rank: round_to_2(average_rank),
                rankings_count: values.len(),
            })
        })
        .collect::<Vec<_>>();

    aggregate.sort_by(|left, right| left.average_rank.total_cmp(&right.average_rank));
    aggregate
}

async fn generate_conversation_title(
    client: &Client,
    config: Arc<RwLock<CouncilConfig>>,
    user_query: &str,
) -> String {
    let title_prompt = format!(
        "请为以下问题生成一个非常简短的中文标题（最多5个汉字，如有可能请不要超过8个字）。\n标题必须简洁且具有概括性，不要包含任何标点符号或引号。\n\n问题：{user_query}\n\n标题："
    );

    let messages = vec![ChatMessage::text("user", title_prompt)];

    let chairman_model = {
        let cfg = config.read().unwrap();
        cfg.chairman_model.clone()
    }; // cfg dropped

    let response = query_model(client, &config, &chairman_model, &messages, 30.0).await;

    let mut title = response
        .map(|reply| reply.content)
        .unwrap_or_else(|| "新对话".to_string())
        .replace(['\r', '\n'], " ")
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string();

    if title.is_empty() {
        title = "新对话".to_string();
    }

    if title.len() > 50 {
        title.truncate(47);
        title.push_str("...");
    }

    title
}

async fn run_full_council(
    client: &Client,
    config: Arc<RwLock<CouncilConfig>>,
    history: &[ChatMessage],
    user_query: &str,
) -> SendMessageResponse {
    let council_enabled = { config.read().unwrap().council_enabled };

    // Council disabled: answer directly with the Chairman, skipping stages 2 & 3.
    if !council_enabled {
        let stage3 = chairman_only_response(client, config.clone(), history, user_query).await;
        return SendMessageResponse {
            stage1: Vec::new(),
            stage2: Vec::new(),
            stage3,
            metadata: CouncilMetadata::default(),
        };
    }

    let stage1 = stage1_collect_responses(client, config.clone(), history, user_query).await;

    if stage1.is_empty() {
        return SendMessageResponse {
            stage1,
            stage2: Vec::new(),
            stage3: Stage3Response {
                model: "error".to_string(),
                response: "All models failed to respond. Please try again.".to_string(),
            },
            metadata: CouncilMetadata::default(),
        };
    }

    let (stage2, label_to_model) =
        stage2_collect_rankings(client, config.clone(), history, user_query, &stage1).await;
    let aggregate_rankings = calculate_aggregate_rankings(&stage2, &label_to_model);
    let stage3 =
        stage3_synthesize_final(client, config.clone(), history, user_query, &stage1, &stage2).await;

    SendMessageResponse {
        stage1,
        stage2,
        stage3,
        metadata: CouncilMetadata {
            label_to_model,
            aggregate_rankings,
        },
    }
}

// ── Conversation storage ──────────────────────────────────────────────

async fn conversation_directory(
    app_handle: &AppHandle,
    config: &Arc<RwLock<CouncilConfig>>,
) -> Result<PathBuf, String> {
    let directory = {
        let cfg = config.read().unwrap();
        if let Some(data_dir) = &cfg.data_dir {
            PathBuf::from(data_dir)
        } else {
            let mut dir = app_handle
                .path()
                .app_data_dir()
                .map_err(|error| error.to_string())?;
            dir.push("conversations");
            dir
        }
    }; // cfg dropped here

    tokio::fs::create_dir_all(&directory)
        .await
        .map_err(|error| error.to_string())?;
    Ok(directory)
}

async fn conversation_path(
    app_handle: &AppHandle,
    config: &Arc<RwLock<CouncilConfig>>,
    conversation_id: &str,
) -> Result<PathBuf, String> {
    let mut path = conversation_directory(app_handle, config).await?;
    path.push(format!("{conversation_id}.json"));
    Ok(path)
}

async fn save_conversation(
    app_handle: &AppHandle,
    config: &Arc<RwLock<CouncilConfig>>,
    conversation: &Conversation,
) -> Result<(), String> {
    let path = conversation_path(app_handle, config, &conversation.id).await?;
    let payload = serde_json::to_vec_pretty(conversation).map_err(|error| error.to_string())?;
    tokio::fs::write(path, payload)
        .await
        .map_err(|error| error.to_string())
}

async fn load_conversation_from_storage(
    app_handle: &AppHandle,
    config: &Arc<RwLock<CouncilConfig>>,
    conversation_id: &str,
) -> Result<Conversation, String> {
    let path = conversation_path(app_handle, config, conversation_id).await?;
    let contents = tokio::fs::read_to_string(path).await.map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            "Conversation not found".to_string()
        } else {
            error.to_string()
        }
    })?;

    serde_json::from_str(&contents).map_err(|error| error.to_string())
}

async fn list_conversations_from_storage(
    app_handle: &AppHandle,
    config: &Arc<RwLock<CouncilConfig>>,
) -> Result<Vec<ConversationMetadata>, String> {
    let directory = conversation_directory(app_handle, config).await?;
    let mut entries = tokio::fs::read_dir(directory)
        .await
        .map_err(|error| error.to_string())?;
    let mut conversations = Vec::new();

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| error.to_string())?
    {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }

        let contents = match tokio::fs::read_to_string(&path).await {
            Ok(contents) => contents,
            Err(error) => {
                eprintln!("Failed to read conversation {:?}: {error}", path);
                continue;
            }
        };

        let conversation = match serde_json::from_str::<Conversation>(&contents) {
            Ok(conversation) => conversation,
            Err(error) => {
                eprintln!("Failed to parse conversation {:?}: {error}", path);
                continue;
            }
        };

        conversations.push(ConversationMetadata {
            id: conversation.id,
            created_at: conversation.created_at,
            title: conversation.title,
            message_count: conversation.messages.len(),
        });
    }

    conversations.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(conversations)
}

async fn create_conversation_in_storage(
    app_handle: &AppHandle,
    config: &Arc<RwLock<CouncilConfig>>,
) -> Result<Conversation, String> {
    let conversation = Conversation {
        id: Uuid::new_v4().to_string(),
        created_at: Utc::now().to_rfc3339(),
        title: "新对话".to_string(),
        messages: Vec::new(),
    };

    save_conversation(app_handle, config, &conversation).await?;
    Ok(conversation)
}

async fn update_conversation_title(
    app_handle: &AppHandle,
    config: &Arc<RwLock<CouncilConfig>>,
    conversation_id: &str,
    title: String,
) -> Result<(), String> {
    let mut conversation =
        load_conversation_from_storage(app_handle, config, conversation_id).await?;
    conversation.title = title;
    save_conversation(app_handle, config, &conversation).await
}

async fn delete_conversation_from_storage(
    app_handle: &AppHandle,
    config: &Arc<RwLock<CouncilConfig>>,
    conversation_id: &str,
) -> Result<(), String> {
    let path = conversation_path(app_handle, config, conversation_id).await?;
    if !path.exists() {
        return Err("Conversation not found".to_string());
    }
    tokio::fs::remove_file(path)
        .await
        .map_err(|error| error.to_string())
}

async fn append_user_message(
    app_handle: &AppHandle,
    config: &Arc<RwLock<CouncilConfig>>,
    conversation_id: &str,
    content: String,
    images: Vec<String>,
) -> Result<(), String> {
    let mut conversation =
        load_conversation_from_storage(app_handle, config, conversation_id).await?;
    conversation.messages.push(user_message(content, images));
    save_conversation(app_handle, config, &conversation).await
}

async fn append_assistant_message(
    app_handle: &AppHandle,
    config: &Arc<RwLock<CouncilConfig>>,
    conversation_id: &str,
    response: &SendMessageResponse,
) -> Result<(), String> {
    let mut conversation =
        load_conversation_from_storage(app_handle, config, conversation_id).await?;
    conversation
        .messages
        .push(assistant_message_from_response(response));
    save_conversation(app_handle, config, &conversation).await
}

fn emit_stream_event(window: &WebviewWindow, event: StreamEvent) -> Result<(), String> {
    window
        .emit(STREAM_EVENT_NAME, event)
        .map_err(|error| error.to_string())
}

// ── Streaming council ──────────────────────────────────────────────────

async fn run_streaming_council(
    window: WebviewWindow,
    app_handle: AppHandle,
    client: Client,
    config: Arc<RwLock<CouncilConfig>>,
    conversation_id: String,
    content: String,
    images: Vec<String>,
) -> Result<(), String> {
    let conversation =
        load_conversation_from_storage(&app_handle, &config, &conversation_id).await?;
    let is_first_message = conversation.messages.is_empty();
    
    let history = extract_conversation_history(&conversation, 3);

    append_user_message(
        &app_handle,
        &config,
        &conversation_id,
        content.clone(),
        images.clone(),
    )
    .await?;

    let title_task = if is_first_message {
        let client = client.clone();
        let config = config.clone();
        let content = content.clone();
        Some(tauri::async_runtime::spawn(async move {
            generate_conversation_title(&client, config, &content).await
        }))
    } else {
        None
    };

    let council_enabled = { config.read().unwrap().council_enabled };
    let has_images = !images.is_empty();

    // When images are attached, bypass the council entirely and answer directly
    // with the dedicated image model. Otherwise, when the council feature is
    // disabled, skip stages 2 & 3 and answer with the Chairman directly.
    let (stage1, stage2, metadata, stage3) = if has_images {
        emit_stream_event(
            &window,
            StreamEvent::new(&conversation_id, "stage1_complete").with_data(json!([])),
        )?;

        let empty_metadata = CouncilMetadata::default();
        emit_stream_event(
            &window,
            StreamEvent::new(&conversation_id, "stage2_complete")
                .with_data(json!([]))
                .with_metadata(empty_metadata.clone()),
        )?;

        emit_stream_event(&window, StreamEvent::new(&conversation_id, "stage3_start"))?;
        let stage3 =
            image_analysis_response(&client, config.clone(), &history, &content, &images).await;
        emit_stream_event(
            &window,
            StreamEvent::new(&conversation_id, "stage3_complete")
                .with_data(serialize_value(&stage3)?),
        )?;

        (Vec::new(), Vec::new(), empty_metadata, stage3)
    } else if !council_enabled {
        // Skip stages 1 & 2 entirely. Don't emit their "start" events so the UI
        // doesn't flash "collecting responses / peer review" spinners; just send
        // empty completion payloads to keep the frontend state machine in sync.
        emit_stream_event(
            &window,
            StreamEvent::new(&conversation_id, "stage1_complete").with_data(json!([])),
        )?;

        let empty_metadata = CouncilMetadata::default();
        emit_stream_event(
            &window,
            StreamEvent::new(&conversation_id, "stage2_complete")
                .with_data(json!([]))
                .with_metadata(empty_metadata.clone()),
        )?;

        emit_stream_event(&window, StreamEvent::new(&conversation_id, "stage3_start"))?;
        let stage3 = chairman_only_response(&client, config.clone(), &history, &content).await;
        emit_stream_event(
            &window,
            StreamEvent::new(&conversation_id, "stage3_complete")
                .with_data(serialize_value(&stage3)?),
        )?;

        (Vec::new(), Vec::new(), empty_metadata, stage3)
    } else {
        emit_stream_event(&window, StreamEvent::new(&conversation_id, "stage1_start"))?;
        let stage1 = stage1_collect_responses(&client, config.clone(), &history, &content).await;
        emit_stream_event(
            &window,
            StreamEvent::new(&conversation_id, "stage1_complete")
                .with_data(serialize_value(&stage1)?),
        )?;

        let (stage2, metadata, stage3) = if stage1.is_empty() {
            let empty_metadata = CouncilMetadata::default();
            emit_stream_event(&window, StreamEvent::new(&conversation_id, "stage2_start"))?;
            emit_stream_event(
                &window,
                StreamEvent::new(&conversation_id, "stage2_complete")
                    .with_data(json!([]))
                    .with_metadata(empty_metadata.clone()),
            )?;
            emit_stream_event(&window, StreamEvent::new(&conversation_id, "stage3_start"))?;
            let stage3 = Stage3Response {
                model: "error".to_string(),
                response: "All models failed to respond. Please try again.".to_string(),
            };
            emit_stream_event(
                &window,
                StreamEvent::new(&conversation_id, "stage3_complete")
                    .with_data(serialize_value(&stage3)?),
            )?;
            (Vec::new(), empty_metadata, stage3)
        } else {
            emit_stream_event(&window, StreamEvent::new(&conversation_id, "stage2_start"))?;
            let (stage2, label_to_model) =
                stage2_collect_rankings(&client, config.clone(), &history, &content, &stage1).await;
            let metadata = CouncilMetadata {
                aggregate_rankings: calculate_aggregate_rankings(&stage2, &label_to_model),
                label_to_model,
            };
            emit_stream_event(
                &window,
                StreamEvent::new(&conversation_id, "stage2_complete")
                    .with_data(serialize_value(&stage2)?)
                    .with_metadata(metadata.clone()),
            )?;

            emit_stream_event(&window, StreamEvent::new(&conversation_id, "stage3_start"))?;
            let stage3 =
                stage3_synthesize_final(&client, config.clone(), &history, &content, &stage1, &stage2)
                    .await;
            emit_stream_event(
                &window,
                StreamEvent::new(&conversation_id, "stage3_complete")
                    .with_data(serialize_value(&stage3)?),
            )?;
            (stage2, metadata, stage3)
        };

        (stage1, stage2, metadata, stage3)
    };

    if let Some(title_task) = title_task {
        if let Ok(title) = title_task.await {
            update_conversation_title(&app_handle, &config, &conversation_id, title.clone())
                .await?;
            emit_stream_event(
                &window,
                StreamEvent::new(&conversation_id, "title_complete")
                    .with_data(json!({ "title": title })),
            )?;
        }
    }

    let response = SendMessageResponse {
        stage1,
        stage2,
        stage3,
        metadata,
    };

    append_assistant_message(&app_handle, &config, &conversation_id, &response).await?;
    emit_stream_event(&window, StreamEvent::new(&conversation_id, "complete"))?;
    Ok(())
}

// ── Tauri commands ────────────────────────────────────────────────────

#[tauri::command]
async fn list_conversations(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<ConversationMetadata>, String> {
    list_conversations_from_storage(&app_handle, &state.config).await
}

#[tauri::command]
async fn create_conversation(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<Conversation, String> {
    create_conversation_in_storage(&app_handle, &state.config).await
}

#[tauri::command]
async fn get_conversation(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<Conversation, String> {
    load_conversation_from_storage(&app_handle, &state.config, &conversation_id).await
}

#[tauri::command]
async fn send_message(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    conversation_id: String,
    content: String,
) -> Result<SendMessageResponse, String> {
    let conversation =
        load_conversation_from_storage(&app_handle, &state.config, &conversation_id).await?;
    let is_first_message = conversation.messages.is_empty();

    let history = extract_conversation_history(&conversation, 3);

    append_user_message(
        &app_handle,
        &state.config,
        &conversation_id,
        content.clone(),
        Vec::new(),
    )
    .await?;

    if is_first_message {
        let title =
            generate_conversation_title(&state.client, state.config.clone(), &content).await;
        update_conversation_title(&app_handle, &state.config, &conversation_id, title).await?;
    }

    let response = run_full_council(&state.client, state.config.clone(), &history, &content).await;
    append_assistant_message(&app_handle, &state.config, &conversation_id, &response).await?;
    Ok(response)
}

#[tauri::command]
fn start_council_stream(
    window: WebviewWindow,
    app_handle: AppHandle,
    state: State<'_, AppState>,
    conversation_id: String,
    content: String,
    images: Option<Vec<String>>,
) -> Result<(), String> {
    let client = state.client.clone();
    let config = state.config.clone();
    let error_window = window.clone();
    let images = images.unwrap_or_default();

    tauri::async_runtime::spawn(async move {
        if let Err(error) = run_streaming_council(
            window,
            app_handle,
            client,
            config,
            conversation_id.clone(),
            content,
            images,
        )
        .await
        {
            let _ = emit_stream_event(
                &error_window,
                StreamEvent::new(&conversation_id, "error").with_message(error),
            );
        }
    });

    Ok(())
}

#[tauri::command]
async fn delete_conversation(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    conversation_id: String,
) -> Result<(), String> {
    delete_conversation_from_storage(&app_handle, &state.config, &conversation_id).await
}

#[tauri::command]
fn get_settings(app_handle: AppHandle) -> Result<AppSettings, String> {
    let settings = load_settings_from_file(&app_handle);
    Ok(settings)
}

#[tauri::command]
fn save_settings(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    // Persist to disk
    save_settings_to_file(&app_handle, &settings)?;

    // Rebuild and swap the runtime config
    let new_config = build_council_config(&settings);
    {
        let mut cfg = state.config.write().unwrap();
        *cfg = new_config;
    }

    Ok(settings)
}

// ── Entry point ───────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let app_handle = app.handle().clone();
            let state = AppState::new(&app_handle);
            app.manage(state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_conversations,
            create_conversation,
            get_conversation,
            send_message,
            delete_conversation,
            start_council_stream,
            get_settings,
            save_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
