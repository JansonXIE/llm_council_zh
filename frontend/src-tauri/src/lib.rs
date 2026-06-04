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

const DEFAULT_COUNCIL_MODELS: &[&str] = &[
    "deepseek/DeepSeek-V4-Pro",
    "minimax/MiniMax-M3",
    "kimi/Kimi-K2.6",
    "glm/GLM-5.1",
];

const DEFAULT_CHAIRMAN_MODEL: &str = "deepseek/DeepSeek-V4-Pro";
const STREAM_EVENT_NAME: &str = "council-event";

// ── Settings ──────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Default)]
struct ModelEntry {
    name: String,
    #[serde(default = "default_true")]
    active: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Serialize, Deserialize, Clone, Default)]
struct AppSettings {
    deepseek_api_key: String,
    deepseek_base_url: String,
    minimax_api_key: String,
    minimax_base_url: String,
    kimi_api_key: String,
    kimi_base_url: String,
    glm_api_key: String,
    glm_base_url: String,
    data_dir: String,
    #[serde(default)]
    models: Vec<ModelEntry>,
    #[serde(default)]
    chairman_model: String,
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
    providers: HashMap<String, ProviderConfig>,
    data_dir: Option<String>,
}

#[derive(Clone)]
struct ProviderConfig {
    api_key: Option<String>,
    base_url: Option<String>,
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

fn chat_completions_base_url(env_name: &str) -> Option<String> {
    let base = env_var(env_name)?;
    let trimmed = base.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.ends_with("/chat/completions") {
        Some(trimmed.to_string())
    } else {
        Some(format!("{trimmed}/chat/completions"))
    }
}

fn apply_base_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim_end_matches('/');
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.ends_with("/chat/completions") {
        Some(trimmed.to_string())
    } else {
        Some(format!("{trimmed}/chat/completions"))
    }
}

fn build_council_config(settings: &AppSettings) -> CouncilConfig {
    dotenvy::dotenv().ok();

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
            // Use settings.models (active ones) if present; otherwise use hardcoded defaults
            if settings.models.is_empty() {
                DEFAULT_COUNCIL_MODELS
                    .iter()
                    .map(|model| (*model).to_string())
                    .collect()
            } else {
                settings
                    .models
                    .iter()
                    .filter(|entry| entry.active)
                    .map(|entry| entry.name.clone())
                    .collect()
            }
        });

    let chairman_model = env_var("CHAIRMAN_MODEL")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| {
            if !settings.chairman_model.is_empty() {
                settings.chairman_model.clone()
            } else {
                DEFAULT_CHAIRMAN_MODEL.to_string()
            }
        });

    // Helper: prefer settings value, fall back to env var
    fn pick_key(settings_val: &str, env_name: &str) -> Option<String> {
        if !settings_val.is_empty() {
            Some(settings_val.to_string())
        } else {
            env_var(env_name)
        }
    }

    fn pick_url(settings_val: &str, env_name: &str) -> Option<String> {
        if !settings_val.is_empty() {
            apply_base_url(settings_val)
        } else {
            chat_completions_base_url(env_name)
        }
    }

    let mut providers = HashMap::new();
    providers.insert(
        "deepseek".to_string(),
        ProviderConfig {
            api_key: pick_key(&settings.deepseek_api_key, "DEEPSEEK_API_KEY"),
            base_url: pick_url(&settings.deepseek_base_url, "DEEPSEEK_BASE_URL"),
        },
    );
    providers.insert(
        "minimax".to_string(),
        ProviderConfig {
            api_key: pick_key(&settings.minimax_api_key, "MINIMAX_API_KEY"),
            base_url: pick_url(&settings.minimax_base_url, "MINIMAX_BASE_URL"),
        },
    );
    providers.insert(
        "kimi".to_string(),
        ProviderConfig {
            api_key: pick_key(&settings.kimi_api_key, "KIMI_API_KEY"),
            base_url: pick_url(&settings.kimi_base_url, "KIMI_BASE_URL"),
        },
    );
    providers.insert(
        "glm".to_string(),
        ProviderConfig {
            api_key: pick_key(&settings.glm_api_key, "GLM_API_KEY"),
            base_url: pick_url(&settings.glm_base_url, "GLM_BASE_URL"),
        },
    );
    providers.insert(
        "openrouter".to_string(),
        ProviderConfig {
            api_key: env_var("OPENROUTER_API_KEY"),
            base_url: Some("https://openrouter.ai/api/v1/chat/completions".to_string()),
        },
    );

    // Data directory: prefer settings, fall back to default
    let data_dir = if !settings.data_dir.is_empty() {
        Some(settings.data_dir.clone())
    } else {
        None
    };

    CouncilConfig {
        chairman_model,
        council_models,
        providers,
        data_dir,
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
        stage1: Some(response.stage1.clone()),
        stage2: Some(response.stage2.clone()),
        stage3: Some(response.stage3.clone()),
        metadata: Some(response.metadata.clone()),
    }
}

fn user_message(content: String) -> ConversationMessage {
    ConversationMessage {
        role: "user".to_string(),
        content: Some(content),
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

fn serialize_value<T: Serialize>(value: &T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|error| error.to_string())
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
    let (api_key, api_url, request_model) = {
        let cfg = config.read().unwrap();

        let (provider_name, provider_model_id) = model
            .split_once('/')
            .map(|(provider, actual)| (provider.to_lowercase(), actual.to_string()))
            .unwrap_or_else(|| ("openrouter".to_string(), model.to_string()));

        let provider = cfg
            .providers
            .get(&provider_name)
            .filter(|provider| provider.api_key.is_some() && provider.base_url.is_some());

        if let Some(provider) = provider {
            (
                provider.api_key.clone().unwrap_or_default(),
                provider.base_url.clone().unwrap_or_default(),
                provider_model_id,
            )
        } else {
            let fallback = cfg.providers.get("openrouter")?;
            (
                fallback.api_key.clone()?,
                fallback.base_url.clone()?,
                model.to_string(),
            )
        }
    }; // cfg dropped here, before any .await

    let response = client
        .post(api_url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .timeout(Duration::from_secs_f64(timeout_secs))
        .json(&json!({
            "model": request_model,
            "messages": messages,
        }))
        .send()
        .await;

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

    Some(ModelReply {
        content: extract_content(&data),
    })
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
    user_query: &str,
) -> Vec<Stage1Response> {
    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: user_query.to_string(),
    }];

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

    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: ranking_prompt,
    }];

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

    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: chairman_prompt,
    }];

    let chairman_model = {
        let cfg = config.read().unwrap();
        cfg.chairman_model.clone()
    }; // cfg dropped

    let response = query_model(
        client,
        &config,
        &chairman_model,
        &messages,
        120.0,
    )
    .await;

    Stage3Response {
        model: chairman_model,
        response: response
            .map(|reply| reply.content)
            .unwrap_or_else(|| "Error: Unable to generate final synthesis.".to_string()),
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
                .filter_map(|capture| pattern.find(capture.as_str()).map(|value| value.as_str().to_string()))
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
        "Generate a very short title (3-5 words maximum) that summarizes the following question.\nThe title should be concise and descriptive. Do not use quotes or punctuation in the title.\n\nQuestion: {user_query}\n\nTitle:"
    );

    let messages = vec![ChatMessage {
        role: "user".to_string(),
        content: title_prompt,
    }];

    let chairman_model = {
        let cfg = config.read().unwrap();
        cfg.chairman_model.clone()
    }; // cfg dropped

    let response = query_model(
        client,
        &config,
        &chairman_model,
        &messages,
        30.0,
    )
    .await;

    let mut title = response
        .map(|reply| reply.content)
        .unwrap_or_else(|| "New Conversation".to_string())
        .replace(['\r', '\n'], " ")
        .trim()
        .trim_matches('"')
        .trim_matches('\'')
        .to_string();

    if title.is_empty() {
        title = "New Conversation".to_string();
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
    user_query: &str,
) -> SendMessageResponse {
    let stage1 = stage1_collect_responses(client, config.clone(), user_query).await;

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

    let (stage2, label_to_model) = stage2_collect_rankings(client, config.clone(), user_query, &stage1).await;
    let aggregate_rankings = calculate_aggregate_rankings(&stage2, &label_to_model);
    let stage3 = stage3_synthesize_final(client, config.clone(), user_query, &stage1, &stage2).await;

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

async fn conversation_directory(app_handle: &AppHandle, config: &Arc<RwLock<CouncilConfig>>) -> Result<PathBuf, String> {
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

async fn conversation_path(app_handle: &AppHandle, config: &Arc<RwLock<CouncilConfig>>, conversation_id: &str) -> Result<PathBuf, String> {
    let mut path = conversation_directory(app_handle, config).await?;
    path.push(format!("{conversation_id}.json"));
    Ok(path)
}

async fn save_conversation(app_handle: &AppHandle, config: &Arc<RwLock<CouncilConfig>>, conversation: &Conversation) -> Result<(), String> {
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

    while let Some(entry) = entries.next_entry().await.map_err(|error| error.to_string())? {
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
        title: "New Conversation".to_string(),
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
    let mut conversation = load_conversation_from_storage(app_handle, config, conversation_id).await?;
    conversation.title = title;
    save_conversation(app_handle, config, &conversation).await
}

async fn append_user_message(
    app_handle: &AppHandle,
    config: &Arc<RwLock<CouncilConfig>>,
    conversation_id: &str,
    content: String,
) -> Result<(), String> {
    let mut conversation = load_conversation_from_storage(app_handle, config, conversation_id).await?;
    conversation.messages.push(user_message(content));
    save_conversation(app_handle, config, &conversation).await
}

async fn append_assistant_message(
    app_handle: &AppHandle,
    config: &Arc<RwLock<CouncilConfig>>,
    conversation_id: &str,
    response: &SendMessageResponse,
) -> Result<(), String> {
    let mut conversation = load_conversation_from_storage(app_handle, config, conversation_id).await?;
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
) -> Result<(), String> {
    let conversation = load_conversation_from_storage(&app_handle, &config, &conversation_id).await?;
    let is_first_message = conversation.messages.is_empty();

    append_user_message(&app_handle, &config, &conversation_id, content.clone()).await?;

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

    emit_stream_event(&window, StreamEvent::new(&conversation_id, "stage1_start"))?;
    let stage1 = stage1_collect_responses(&client, config.clone(), &content).await;
    emit_stream_event(
        &window,
        StreamEvent::new(&conversation_id, "stage1_complete").with_data(serialize_value(&stage1)?),
    )?;

    let (stage2, metadata, stage3) = if stage1.is_empty() {
        let empty_metadata = CouncilMetadata::default();
        emit_stream_event(
            &window,
            StreamEvent::new(&conversation_id, "stage2_start"),
        )?;
        emit_stream_event(
            &window,
            StreamEvent::new(&conversation_id, "stage2_complete")
                .with_data(json!([]))
                .with_metadata(empty_metadata.clone()),
        )?;
        emit_stream_event(
            &window,
            StreamEvent::new(&conversation_id, "stage3_start"),
        )?;
        let stage3 = Stage3Response {
            model: "error".to_string(),
            response: "All models failed to respond. Please try again.".to_string(),
        };
        emit_stream_event(
            &window,
            StreamEvent::new(&conversation_id, "stage3_complete").with_data(serialize_value(&stage3)?),
        )?;
        (Vec::new(), empty_metadata, stage3)
    } else {
        emit_stream_event(
            &window,
            StreamEvent::new(&conversation_id, "stage2_start"),
        )?;
        let (stage2, label_to_model) = stage2_collect_rankings(&client, config.clone(), &content, &stage1).await;
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

        emit_stream_event(
            &window,
            StreamEvent::new(&conversation_id, "stage3_start"),
        )?;
        let stage3 = stage3_synthesize_final(&client, config.clone(), &content, &stage1, &stage2).await;
        emit_stream_event(
            &window,
            StreamEvent::new(&conversation_id, "stage3_complete").with_data(serialize_value(&stage3)?),
        )?;
        (stage2, metadata, stage3)
    };

    if let Some(title_task) = title_task {
        if let Ok(title) = title_task.await {
            update_conversation_title(&app_handle, &config, &conversation_id, title.clone()).await?;
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
async fn list_conversations(app_handle: AppHandle, state: State<'_, AppState>) -> Result<Vec<ConversationMetadata>, String> {
    list_conversations_from_storage(&app_handle, &state.config).await
}

#[tauri::command]
async fn create_conversation(app_handle: AppHandle, state: State<'_, AppState>) -> Result<Conversation, String> {
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
    let conversation = load_conversation_from_storage(&app_handle, &state.config, &conversation_id).await?;
    let is_first_message = conversation.messages.is_empty();

    append_user_message(&app_handle, &state.config, &conversation_id, content.clone()).await?;

    if is_first_message {
        let title = generate_conversation_title(&state.client, state.config.clone(), &content).await;
        update_conversation_title(&app_handle, &state.config, &conversation_id, title).await?;
    }

    let response = run_full_council(&state.client, state.config.clone(), &content).await;
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
) -> Result<(), String> {
    let client = state.client.clone();
    let config = state.config.clone();
    let error_window = window.clone();

    tauri::async_runtime::spawn(async move {
        if let Err(error) = run_streaming_council(
            window,
            app_handle,
            client,
            config,
            conversation_id.clone(),
            content,
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
fn get_settings(app_handle: AppHandle) -> Result<AppSettings, String> {
    let mut settings = load_settings_from_file(&app_handle);

    // Hydrate defaults: if models is empty, populate with DEFAULT_COUNCIL_MODELS (all active)
    if settings.models.is_empty() {
        settings.models = DEFAULT_COUNCIL_MODELS
            .iter()
            .map(|name| ModelEntry {
                name: (*name).to_string(),
                active: true,
            })
            .collect();
    }

    // Hydrate defaults: if chairman_model is empty, use DEFAULT_CHAIRMAN_MODEL
    if settings.chairman_model.is_empty() {
        settings.chairman_model = DEFAULT_CHAIRMAN_MODEL.to_string();
    }

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
            start_council_stream,
            get_settings,
            save_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}