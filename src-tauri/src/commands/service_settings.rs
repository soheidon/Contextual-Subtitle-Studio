use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::llm::{provider_preset, ModelTier};
use crate::log::emit_log;

const DEFAULT_TMDB_ENV_VAR: &str = "TMDB_API_KEY";
const DEFAULT_TMDB_BASE_URL: &str = "https://api.themoviedb.org";
const DEFAULT_SRT_EN_PATTERN: &str = r"_en\.srt$";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Per-provider overrides stored under `providers.{prefix}` in settings.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pro_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flash_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_tier: Option<ModelTier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supports_thinking: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking: Option<String>,
}

/// Returned to the UI.  All fields are resolved (defaults applied on the Rust side).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedProviderSettings {
    pub base_url: String,
    pub model: String,
    pub pro_model: String,
    pub flash_model: String,
    pub default_tier: ModelTier,
    pub supports_thinking: bool,
    pub thinking: String,
}

/// Model-tier choices for each LLM-backed workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmTaskModelSettings {
    #[serde(default = "default_tier_pro")]
    pub synopsis_generation: ModelTier,
    #[serde(default = "default_tier_pro")]
    pub scene_detection: ModelTier,
    #[serde(default = "default_tier_pro")]
    pub scene_context_analysis: ModelTier,
    #[serde(default = "default_tier_pro")]
    pub proper_noun_confirmation: ModelTier,
    #[serde(default = "default_tier_pro")]
    pub subtitle_translation: ModelTier,
    #[serde(default = "default_tier_flash")]
    pub lightweight_cleanup: ModelTier,
    #[serde(default = "default_tier_pro")]
    pub kanji_correction: ModelTier,
    #[serde(default = "default_tier_flash")]
    pub zh_context_disambiguation: ModelTier,
}

fn default_tier_pro() -> ModelTier {
    ModelTier::Pro
}

fn default_tier_flash() -> ModelTier {
    ModelTier::Flash
}

impl Default for LlmTaskModelSettings {
    fn default() -> Self {
        Self {
            synopsis_generation: ModelTier::Pro,
            scene_detection: ModelTier::Pro,
            scene_context_analysis: ModelTier::Pro,
            proper_noun_confirmation: ModelTier::Pro,
            subtitle_translation: ModelTier::Pro,
            lightweight_cleanup: ModelTier::Flash,
            kanji_correction: ModelTier::Pro,
            zh_context_disambiguation: ModelTier::Flash,
        }
    }
}

/// TMDb-only settings (kept separate from LLM provider settings).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSettings {
    pub tmdb_env_var_name: String,
    pub tmdb_base_url: String,
    #[serde(default = "default_srt_en_pattern")]
    pub srt_en_pattern: String,
}

fn default_srt_en_pattern() -> String {
    DEFAULT_SRT_EN_PATTERN.to_string()
}

impl Default for ServiceSettings {
    fn default() -> Self {
        Self {
            tmdb_env_var_name: DEFAULT_TMDB_ENV_VAR.to_string(),
            tmdb_base_url: DEFAULT_TMDB_BASE_URL.to_string(),
            srt_en_pattern: DEFAULT_SRT_EN_PATTERN.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn preset_for_prefix(prefix: &str) -> crate::llm::ProviderPreset {
    provider_preset(&format!("{}_API_KEY", prefix)).unwrap_or(crate::llm::ProviderPreset {
        provider: prefix.to_string(),
        base_url: String::new(),
        pro_model: String::new(),
        flash_model: String::new(),
        default_tier: ModelTier::Pro,
        supports_thinking: false,
        default_thinking: "disabled".to_string(),
    })
}

fn settings_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

fn read_settings(app: &tauri::AppHandle) -> serde_json::Value {
    let Ok(path) = settings_path(app) else {
        return serde_json::json!({});
    };
    if !path.exists() {
        return serde_json::json!({});
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(serde_json::json!({}))
}

fn write_settings(app: &tauri::AppHandle, settings: &serde_json::Value) -> Result<(), String> {
    let path = settings_path(app)?;
    let json = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Migration from old flat keys → providers map
// ---------------------------------------------------------------------------

fn migrate_old_keys(app: &tauri::AppHandle, settings: &mut serde_json::Value) {
    let mut changed = false;
    let mut ds_settings = serde_json::json!({});

    // Copy old flat keys if present
    if let Some(v) = settings.get("deepseek_base_url").and_then(|v| v.as_str()) {
        ds_settings["base_url"] = serde_json::Value::String(v.to_string());
        changed = true;
    }
    if let Some(v) = settings.get("deepseek_model").and_then(|v| v.as_str()) {
        ds_settings["model"] = serde_json::Value::String(v.to_string());
        changed = true;
    }
    if let Some(v) = settings.get("deepseek_thinking").and_then(|v| v.as_str()) {
        ds_settings["thinking"] = serde_json::Value::String(v.to_string());
        changed = true;
    }

    if changed {
        settings["providers"] = serde_json::json!({ "DEEPSEEK": ds_settings });
        // Remove old keys
        if let Some(obj) = settings.as_object_mut() {
            obj.remove("deepseek_base_url");
            obj.remove("deepseek_model");
            obj.remove("deepseek_thinking");
        }
        eprintln!("[ServiceSettings] 旧形式のDeepSeek設定を providers.DEEPSEEK に移行しました");
        let _ = write_settings(app, settings);
    }
}

// ---------------------------------------------------------------------------
// DeepSeek model-name migration (deepseek-chat / deepseek-reasoner → V4)
// ---------------------------------------------------------------------------

fn migrate_deepseek_model(model: &str, thinking: &str) -> (String, String) {
    match model {
        "deepseek-chat" => {
            eprintln!("[DeepSeek] 旧モデル名 deepseek-chat を deepseek-v4-flash に移行しました");
            ("deepseek-v4-flash".to_string(), "disabled".to_string())
        }
        "deepseek-reasoner" => {
            eprintln!("[DeepSeek] 旧モデル名 deepseek-reasoner を deepseek-v4-pro + thinking enabled に移行しました");
            ("deepseek-v4-pro".to_string(), "enabled".to_string())
        }
        _ => (model.to_string(), thinking.to_string()),
    }
}

fn looks_like_flash_model(model: &str) -> bool {
    let model = model.to_ascii_lowercase();
    model.contains("flash") || model.contains("haiku") || model.ends_with("-8k")
}

fn parse_model_tier(value: Option<&str>, default_tier: ModelTier) -> ModelTier {
    match value.unwrap_or_default().to_ascii_lowercase().as_str() {
        "flash" => ModelTier::Flash,
        "pro" => ModelTier::Pro,
        _ => default_tier,
    }
}

// ---------------------------------------------------------------------------
// Public helpers (used by commands/llm.rs, commands/srt.rs)
// ---------------------------------------------------------------------------

/// Read the configured SRT English-filename regex from settings.
pub fn read_srt_en_pattern(app: &tauri::AppHandle) -> String {
    let settings = read_settings(app);
    settings["srt_en_pattern"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| DEFAULT_SRT_EN_PATTERN.to_string())
}

/// Read model-tier choices for LLM-backed workflows.
pub fn read_llm_task_model_settings(app: &tauri::AppHandle) -> LlmTaskModelSettings {
    let settings = read_settings(app);
    serde_json::from_value(settings["llm_task_model_settings"].clone()).unwrap_or_default()
}

/// Read the resolved per-provider settings from disk.
/// Applies defaults, migration, and old-model-name migration for DeepSeek.
pub fn read_provider_settings(app: &tauri::AppHandle, prefix: &str) -> ResolvedProviderSettings {
    let mut settings = read_settings(app);
    migrate_old_keys(app, &mut settings);
    migrate_gemini_defaults(app, &mut settings);
    migrate_openai_defaults(app, &mut settings);

    let providers = &settings["providers"];
    let ps = &providers[prefix];

    let preset = preset_for_prefix(prefix);
    let default_url = preset.base_url.as_str();
    let default_thinking = preset.default_thinking.as_str();
    let supports_thinking = ps["supports_thinking"]
        .as_bool()
        .unwrap_or(preset.supports_thinking);

    let raw_thinking = ps["thinking"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or(default_thinking);

    let legacy_model = ps["model"].as_str().filter(|s| !s.is_empty());
    let mut pro_model = ps["pro_model"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or(&preset.pro_model)
        .to_string();
    let mut flash_model = ps["flash_model"]
        .as_str()
        .filter(|s| !s.is_empty())
        .unwrap_or(&preset.flash_model)
        .to_string();
    let mut default_tier = parse_model_tier(ps["default_tier"].as_str(), preset.default_tier);
    let mut thinking = raw_thinking.to_string();

    if let Some(model) = legacy_model {
        let (migrated_model, migrated_thinking) = if prefix == "DEEPSEEK" {
            migrate_deepseek_model(model, raw_thinking)
        } else {
            (model.to_string(), raw_thinking.to_string())
        };

        thinking = migrated_thinking;
        if looks_like_flash_model(&migrated_model) {
            flash_model = migrated_model;
            default_tier = ModelTier::Flash;
        } else {
            pro_model = migrated_model;
            default_tier = ModelTier::Pro;
        }
    }

    if prefix == "DEEPSEEK" {
        let (migrated_pro, migrated_thinking) = migrate_deepseek_model(&pro_model, &thinking);
        let (migrated_flash, _) = migrate_deepseek_model(&flash_model, "disabled");
        pro_model = migrated_pro;
        flash_model = migrated_flash;
        thinking = migrated_thinking;
    }

    let model = match default_tier {
        ModelTier::Pro => pro_model.clone(),
        ModelTier::Flash => flash_model.clone(),
    };

    ResolvedProviderSettings {
        base_url: ps["base_url"]
            .as_str()
            .filter(|s| !s.is_empty())
            .unwrap_or(default_url)
            .to_string(),
        model,
        pro_model,
        flash_model,
        default_tier,
        supports_thinking,
        thinking,
    }
}

// ---------------------------------------------------------------------------
// Gemini defaults migration (normalize old saved values)
// ---------------------------------------------------------------------------

/// If a saved Gemini provider setting matches a known old default,
/// update it in-place to the current default and persist.
fn migrate_gemini_defaults(app: &tauri::AppHandle, settings: &mut serde_json::Value) {
    if settings["providers"].is_null() || !settings["providers"]["GEMINI"].is_object() {
        return;
    }

    let gemini = &settings["providers"]["GEMINI"];
    let mut changed = false;
    let mut migrated = serde_json::json!({});

    // Copy existing values
    if let Some(obj) = gemini.as_object() {
        for (key, value) in obj {
            migrated[key] = value.clone();
        }
    }

    // Normalize: base_url without trailing slash → with trailing slash
    if let Some(url) = migrated["base_url"].as_str() {
        if url == "https://generativelanguage.googleapis.com/v1beta/openai" {
            migrated["base_url"] =
                serde_json::json!("https://generativelanguage.googleapis.com/v1beta/openai/");
            eprintln!("[Gemini] base_url を正規化: 末尾 / を追加しました");
            changed = true;
        }
    }

    // Normalize: old default model gemini-2.0-flash → gemini-3.5-flash
    // Only fix exact old default — don't touch user-set models like gemini-2.5-flash
    if let Some(model) = migrated["model"].as_str() {
        if model == "gemini-2.0-flash" {
            migrated["model"] = serde_json::json!("gemini-3.5-flash");
            eprintln!("[Gemini] model を gemini-2.0-flash → gemini-3.5-flash に移行しました");
            changed = true;
        }
    }

    if changed {
        settings["providers"]["GEMINI"] = migrated;
        let _ = write_settings(app, settings);
    }
}

// ---------------------------------------------------------------------------
// OpenAI defaults migration (normalize old saved values)
// ---------------------------------------------------------------------------

/// If a saved OpenAI provider setting matches a known old default,
/// update it in-place to the current default and persist.
fn migrate_openai_defaults(app: &tauri::AppHandle, settings: &mut serde_json::Value) {
    if settings["providers"].is_null() || !settings["providers"]["OPENAI"].is_object() {
        return;
    }

    let openai = &settings["providers"]["OPENAI"];
    let mut changed = false;
    let mut migrated = serde_json::json!({});

    // Copy existing values
    if let Some(obj) = openai.as_object() {
        for (key, value) in obj {
            migrated[key] = value.clone();
        }
    }

    // Normalize old base_url without /v1 → with /v1
    if let Some(url) = migrated["base_url"].as_str() {
        if url == "https://api.openai.com" {
            migrated["base_url"] = serde_json::json!("https://api.openai.com/v1");
            eprintln!(
                "[OpenAI] base_url を正規化: https://api.openai.com → https://api.openai.com/v1"
            );
            changed = true;
        }
    }

    // Normalize old default model gpt-4o-mini → gpt-5.5
    if let Some(model) = migrated["model"].as_str() {
        if model == "gpt-4o-mini" {
            migrated["model"] = serde_json::json!("gpt-5.5");
            eprintln!("[OpenAI] model を gpt-4o-mini → gpt-5.5 に移行しました");
            changed = true;
        }
    }

    if changed {
        settings["providers"]["OPENAI"] = migrated;
        let _ = write_settings(app, settings);
    }
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn get_service_settings(app: tauri::AppHandle) -> ServiceSettings {
    let settings = read_settings(&app);
    ServiceSettings {
        tmdb_env_var_name: settings["tmdb_env_var_name"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| DEFAULT_TMDB_ENV_VAR.to_string()),
        tmdb_base_url: settings["tmdb_base_url"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| DEFAULT_TMDB_BASE_URL.to_string()),
        srt_en_pattern: settings["srt_en_pattern"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| DEFAULT_SRT_EN_PATTERN.to_string()),
    }
}

#[tauri::command]
pub fn save_service_settings(
    app: tauri::AppHandle,
    settings: ServiceSettings,
) -> Result<(), String> {
    let mut current = read_settings(&app);
    current["tmdb_env_var_name"] = serde_json::Value::String(settings.tmdb_env_var_name);
    current["tmdb_base_url"] = serde_json::Value::String(settings.tmdb_base_url);
    current["srt_en_pattern"] = serde_json::Value::String(settings.srt_en_pattern);
    write_settings(&app, &current)
}

#[tauri::command]
pub fn get_llm_task_model_settings(app: tauri::AppHandle) -> LlmTaskModelSettings {
    read_llm_task_model_settings(&app)
}

#[tauri::command]
pub fn save_llm_task_model_settings(
    app: tauri::AppHandle,
    settings: LlmTaskModelSettings,
) -> Result<(), String> {
    let mut current = read_settings(&app);
    current["llm_task_model_settings"] = serde_json::json!(settings);
    write_settings(&app, &current)
}

/// Get resolved settings for a single LLM provider.
/// Returns default-filled values so the UI always has something to show.
#[tauri::command]
pub fn get_provider_settings(app: tauri::AppHandle, prefix: String) -> ResolvedProviderSettings {
    read_provider_settings(&app, &prefix)
}

/// Save overrides for a single LLM provider.
/// Only non-empty values are stored; empty means "use default".
#[tauri::command]
pub fn save_provider_settings(
    app: tauri::AppHandle,
    prefix: String,
    settings: ProviderSettings,
) -> Result<(), String> {
    let mut current = read_settings(&app);
    migrate_old_keys(&app, &mut current);

    if current["providers"].is_null() {
        current["providers"] = serde_json::json!({});
    }

    current["providers"][&prefix] = serde_json::json!(settings);
    write_settings(&app, &current)
}

/// Test TMDb API connectivity using the given API key and base URL.
#[tauri::command]
pub async fn test_tmdb_connection(api_key: String, base_url: String) -> Result<bool, String> {
    if api_key.trim().is_empty() {
        return Err("TMDB_API_KEYが未設定です。".to_string());
    }

    let url = format!(
        "{}/3/configuration?api_key={}",
        base_url.trim_end_matches('/'),
        api_key.trim()
    );

    let client = reqwest::Client::new();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("ネットワーク接続に失敗しました: {}", e))?;

    let status = resp.status();
    if status.is_success() {
        Ok(true)
    } else if status.as_u16() == 401 {
        Err("TMDB_API_KEYが無効です。".to_string())
    } else {
        let body = resp.text().await.unwrap_or_default();
        Err(format!("TMDb API HTTP {} — {}", status.as_u16(), body))
    }
}

/// Test OpenAI Responses API connectivity with web_search tool for AI確認.
#[tauri::command]
pub async fn test_openai_ai_confirm(
    app: tauri::AppHandle,
    env_store: tauri::State<'_, crate::envstore::EnvStoreState>,
) -> Result<bool, String> {
    // Resolve OPENAI_API_KEY from env or persistent store
    let api_key = std::env::var("OPENAI_API_KEY")
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| {
            env_store
                .0
                .lock()
                .ok()
                .and_then(|s| s.0.get("OPENAI_API_KEY").cloned())
        })
        .ok_or("OPENAI_API_KEY が設定されていません。設定画面でAPIキーを保存してください。")?;

    // Read the saved model from provider settings
    let overrides = read_provider_settings(&app, "OPENAI");
    let task_models = read_llm_task_model_settings(&app);
    let model = match task_models.proper_noun_confirmation {
        ModelTier::Pro => overrides.pro_model,
        ModelTier::Flash => overrides.flash_model,
    };

    let url = "https://api.openai.com/v1/responses";

    emit_log(&app, "debug", "Service", &format!(
        "OpenAI Responses API: model={} endpoint=https://api.openai.com/v1/responses temperature=omitted json_mode=omitted web_search=on",
        &model
    ));

    let request_body = serde_json::json!({
        "model": &model,
        "input": "Hello",
        "instructions": "Reply with single word OK in JSON: {\"status\":\"ok\"}",
        "tools": [{"type": "web_search"}]
    });

    let client = reqwest::Client::new();
    let response = client
        .post(url)
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("ネットワークエラー: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        emit_log(
            &app,
            "error",
            "Service",
            &format!(
                "OpenAI Responses API HTTP {}: {}",
                status.as_u16(),
                crate::log::preview_chars(&body, 500)
            ),
        );
        return Err(format!(
            "OpenAI Responses API エラー ({}): {}",
            status.as_u16(),
            crate::log::preview_chars(&body, 500)
        ));
    }

    Ok(true)
}
