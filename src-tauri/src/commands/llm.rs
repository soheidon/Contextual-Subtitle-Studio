use crate::commands::project::AppState;
use crate::commands::service_settings;
use crate::envstore::EnvStoreState;
use crate::llm::{provider_preset, all_presets, LlmClient, ProviderConfig, ProviderPreset};
use tauri::{Manager, State};

fn prefix_from_env(name: &str) -> String {
    let name = name.trim().to_uppercase();
    if let Some(p) = name.strip_suffix("_API_KEY") {
        p.to_string()
    } else if let Some(p) = name.strip_suffix("_KEY") {
        p.to_string()
    } else {
        name
    }
}

/// Build a ProviderConfig for a given env var name.
/// Looks up the key from process env first, then from the persistent store.
/// Reads per-provider overrides (base_url, model, thinking) from settings.json for ALL providers.
pub fn build_provider_for(
    name: &str,
    env_store: &EnvStoreState,
    app: &tauri::AppHandle,
) -> Result<ProviderConfig, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("環境変数名が空です。".to_string());
    }
    let api_key = std::env::var(name)
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| {
            env_store
                .0
                .lock()
                .ok()
                .and_then(|s| s.0.get(name).cloned())
        })
        .ok_or_else(|| format!("環境変数 {} の値が見つかりません。", name))?;
    // Verify the env var name maps to a known preset (but we don't use preset defaults).
    let _ = provider_preset(name).ok_or_else(|| {
        format!(
            "環境変数名 {} に対応するプロバイダが見つかりません。対応プレフィックス: DEEPSEEK, OPENAI, ANTHROPIC, MINIMAX, GROQ 等",
            name
        )
    })?;

    let prefix = prefix_from_env(name);
    let overrides = service_settings::read_provider_settings(app, &prefix);

    let thinking = (prefix == "DEEPSEEK").then_some(overrides.thinking);

    // gpt-5.5 rejects temperature; omit for OpenAI, keep 0.3 for others
    let temperature = if prefix == "OPENAI" {
        None
    } else {
        Some(0.3)
    };

    Ok(ProviderConfig {
        provider: "openai_compatible".to_string(),
        base_url: overrides.base_url,
        api_key,
        model: overrides.model,
        thinking,
        temperature,
    })
}

/// Resolve the active env var + preset into a full ProviderConfig.
pub fn resolve_provider(
    state: &AppState,
    env_store: &EnvStoreState,
    app: &tauri::AppHandle,
) -> Result<ProviderConfig, String> {
    let active = state.active_env_var.lock().map_err(|e| e.to_string())?;
    let name = active
        .clone()
        .ok_or("LLMが未設定です。設定画面で環境変数名を保存してください。")?;
    drop(active);
    build_provider_for(&name, env_store, app)
}

#[tauri::command]
pub fn set_provider_config(
    state: State<AppState>,
    config: ProviderConfig,
) -> Result<(), String> {
    let mut stored = state.provider_config.lock().map_err(|e| e.to_string())?;
    *stored = Some(config);
    Ok(())
}

#[tauri::command]
pub fn get_provider_config(
    state: State<AppState>,
) -> Result<Option<ProviderConfig>, String> {
    let config = state.provider_config.lock().map_err(|e| e.to_string())?;
    Ok(config.clone())
}

#[tauri::command]
pub async fn test_llm_connection(config: ProviderConfig) -> Result<bool, String> {
    let client = LlmClient::new(config);
    client.test_connection().await
}

/// Check whether the given env var name (or the saved active one) can connect.
/// `name` is optional - if provided, it is used directly (no need to save first).
#[tauri::command]
pub async fn check_active_connection(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    env_store: State<'_, EnvStoreState>,
    name: Option<String>,
) -> Result<bool, String> {
    let chosen = match name {
        Some(n) if !n.trim().is_empty() => n,
        _ => state
            .active_env_var
            .lock()
            .map_err(|e| e.to_string())?
            .clone()
            .ok_or("環境変数名を入力してください。")?,
    };
    let provider = build_provider_for(&chosen, &env_store, &app)?;
    let client = LlmClient::new(provider);
    client.test_connection().await
}

/// Set which env var name is the "active" one used for translation.
/// Also persists to settings.json so it survives restarts.
#[tauri::command]
pub fn set_active_env_var(
    app: tauri::AppHandle,
    state: State<AppState>,
    name: Option<String>,
) -> Result<(), String> {
    // Persist to settings.json
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("settings.json");
    let mut settings: serde_json::Value = if path.exists() {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };
    settings["active_env_var"] = serde_json::Value::String(name.clone().unwrap_or_default());
    std::fs::write(&path, serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;

    // Update in-memory
    let mut active = state.active_env_var.lock().map_err(|e| e.to_string())?;
    *active = name;
    Ok(())
}

/// Get the active env var name and its masked value (if any).
#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct ActiveEnvVarInfo {
    pub name: Option<String>,
    pub has_key: bool,
    pub provider: Option<String>,
    pub base_url: Option<String>,
    pub model: Option<String>,
}

#[tauri::command]
pub fn get_active_env_var(
    app: tauri::AppHandle,
    state: State<AppState>,
    env_store: State<EnvStoreState>,
) -> Result<ActiveEnvVarInfo, String> {
    let active = state.active_env_var.lock().map_err(|e| e.to_string())?;
    let name = active.clone();
    drop(active);

    if name.is_none() {
        return Ok(ActiveEnvVarInfo {
            name: None,
            has_key: false,
            provider: None,
            base_url: None,
            model: None,
        });
    }
    let name_str = name.clone().unwrap();
    let preset = provider_preset(&name_str);
    let store = env_store.0.lock().map_err(|e| e.to_string())?;
    let has_key = std::env::var(&name_str)
        .ok()
        .filter(|v| !v.is_empty())
        .is_some()
        || store.0.contains_key(&name_str);

    // Use generic per-provider overrides from settings.json
    let prefix = prefix_from_env(&name_str);
    let overrides = service_settings::read_provider_settings(&app, &prefix);

    Ok(ActiveEnvVarInfo {
        name,
        has_key,
        provider: preset.as_ref().map(|p| p.provider.clone()),
        base_url: Some(overrides.base_url),
        model: Some(overrides.model),
    })
}

/// Check if a specific env var name has a key available.
/// Used by the UI to enable the test button when a key is in the system env.
#[tauri::command]
pub fn check_env_var_key_exists(
    env_store: State<EnvStoreState>,
    name: String,
) -> Result<bool, String> {
    if std::env::var(&name)
        .ok()
        .filter(|v| !v.is_empty())
        .is_some()
    {
        return Ok(true);
    }
    let store = env_store.0.lock().map_err(|e| e.to_string())?;
    Ok(store.0.contains_key(&name))
}

/// List all known provider presets (for UI dropdown).
#[tauri::command]
pub fn list_provider_presets() -> Vec<(String, ProviderPreset)> {
    all_presets()
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect()
}
