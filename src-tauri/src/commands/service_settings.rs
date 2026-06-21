use serde::{Deserialize, Serialize};
use tauri::Manager;

const DEFAULT_TMDB_ENV_VAR: &str = "TMDB_API_KEY";
const DEFAULT_TMDB_BASE_URL: &str = "https://api.themoviedb.org";

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
    pub thinking: Option<String>,
}

/// Returned to the UI.  All fields are resolved (defaults applied on the Rust side).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedProviderSettings {
    pub base_url: String,
    pub model: String,
    pub thinking: String,
}

/// TMDb-only settings (kept separate from LLM provider settings).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSettings {
    pub tmdb_env_var_name: String,
    pub tmdb_base_url: String,
}

impl Default for ServiceSettings {
    fn default() -> Self {
        Self {
            tmdb_env_var_name: DEFAULT_TMDB_ENV_VAR.to_string(),
            tmdb_base_url: DEFAULT_TMDB_BASE_URL.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Preset defaults for each known provider prefix.
fn provider_defaults(prefix: &str) -> (&str, &str, &str) {
    match prefix {
        "DEEPSEEK" => ("https://api.deepseek.com", "deepseek-v4-flash", "disabled"),
        "OPENAI" => ("https://api.openai.com", "gpt-4o-mini", "disabled"),
        "ANTHROPIC" | "CLAUDE" => ("https://api.anthropic.com", "claude-sonnet-4-5", "disabled"),
        "GEMINI" | "GOOGLE" => ("https://generativelanguage.googleapis.com/v1beta/openai", "gemini-2.0-flash", "disabled"),
        "MINIMAX" => ("https://api.minimax.chat", "MiniMax-chat", "disabled"),
        "MOONSHOT" | "KIMI" => ("https://api.moonshot.cn", "moonshot-v1-8k", "disabled"),
        _ => ("", "", "disabled"),
    }
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
            eprintln!("[DeepSeek] 旧モデル名 deepseek-reasoner を deepseek-v4-flash + thinking enabled に移行しました");
            ("deepseek-v4-flash".to_string(), "enabled".to_string())
        }
        _ => (model.to_string(), thinking.to_string()),
    }
}

// ---------------------------------------------------------------------------
// Public helpers (used by commands/llm.rs)
// ---------------------------------------------------------------------------

/// Read the resolved per-provider settings from disk.
/// Applies defaults, migration, and old-model-name migration for DeepSeek.
pub fn read_provider_settings(app: &tauri::AppHandle, prefix: &str) -> ResolvedProviderSettings {
    let mut settings = read_settings(app);
    migrate_old_keys(app, &mut settings);

    let providers = &settings["providers"];
    let ps = &providers[prefix];

    let (default_url, default_model, default_thinking) = provider_defaults(prefix);

    let raw_model = ps["model"].as_str().filter(|s| !s.is_empty()).unwrap_or(default_model);
    let raw_thinking = ps["thinking"].as_str().filter(|s| !s.is_empty()).unwrap_or(default_thinking);

    let (model, thinking) = if prefix == "DEEPSEEK" {
        let (m, t) = migrate_deepseek_model(raw_model, raw_thinking);
        // Persist migration
        if m != raw_model || t != raw_thinking {
            let mut current = read_settings(app);
            if current["providers"].is_null() {
                current["providers"] = serde_json::json!({});
            }
            current["providers"][prefix] = serde_json::json!({
                "base_url": ps["base_url"].as_str().filter(|s| !s.is_empty()).unwrap_or(default_url),
                "model": &m,
                "thinking": &t,
            });
            let _ = write_settings(app, &current);
        }
        (m, t)
    } else {
        (raw_model.to_string(), raw_thinking.to_string())
    };

    ResolvedProviderSettings {
        base_url: ps["base_url"].as_str().filter(|s| !s.is_empty()).unwrap_or(default_url).to_string(),
        model,
        thinking,
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
    }
}

#[tauri::command]
pub fn save_service_settings(app: tauri::AppHandle, settings: ServiceSettings) -> Result<(), String> {
    let mut current = read_settings(&app);
    current["tmdb_env_var_name"] = serde_json::Value::String(settings.tmdb_env_var_name);
    current["tmdb_base_url"] = serde_json::Value::String(settings.tmdb_base_url);
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
