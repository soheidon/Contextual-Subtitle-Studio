use serde::{Deserialize, Serialize};

use super::ModelTier;

/// A preset mapping an env var prefix to provider defaults and capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderPreset {
    pub provider: String,
    pub base_url: String,
    pub pro_model: String,
    pub flash_model: String,
    pub default_tier: ModelTier,
    pub supports_thinking: bool,
    pub default_thinking: String,
}

/// Look up the provider preset for a given env var name.
/// Matches by stripping `_API_KEY` (case-insensitive) and looking up the prefix.
pub fn provider_preset(env_var_name: &str) -> Option<ProviderPreset> {
    let name = env_var_name.trim().to_uppercase();
    let prefix = name
        .strip_suffix("_API_KEY")
        .or_else(|| name.strip_suffix("_KEY"))
        .unwrap_or(&name);

    let preset = match prefix {
        "DEEPSEEK" => (
            "DeepSeek",
            "https://api.deepseek.com",
            "deepseek-v4-pro",
            "deepseek-v4-flash",
            ModelTier::Pro,
            true,
            "enabled",
        ),
        "OPENAI" => (
            "OpenAI",
            "https://api.openai.com/v1",
            "gpt-5.5",
            "gpt-5.5",
            ModelTier::Pro,
            false,
            "disabled",
        ),
        "ANTHROPIC" | "CLAUDE" => (
            "Anthropic",
            "https://api.anthropic.com",
            "claude-sonnet-4-5",
            "claude-3-5-haiku-latest",
            ModelTier::Pro,
            false,
            "disabled",
        ),
        "GEMINI" | "GOOGLE" => (
            "Gemini",
            "https://generativelanguage.googleapis.com/v1beta/openai/",
            "gemini-3.5-pro",
            "gemini-3.5-flash",
            ModelTier::Flash,
            false,
            "disabled",
        ),
        "MINIMAX" => (
            "MiniMax",
            "https://api.minimax.chat",
            "MiniMax-chat",
            "MiniMax-chat",
            ModelTier::Flash,
            false,
            "disabled",
        ),
        "MOONSHOT" | "KIMI" => (
            "Kimi / Moonshot",
            "https://api.moonshot.cn",
            "moonshot-v1-32k",
            "moonshot-v1-8k",
            ModelTier::Flash,
            false,
            "disabled",
        ),
        _ => return None,
    };

    Some(ProviderPreset {
        provider: preset.0.to_string(),
        base_url: preset.1.to_string(),
        pro_model: preset.2.to_string(),
        flash_model: preset.3.to_string(),
        default_tier: preset.4,
        supports_thinking: preset.5,
        default_thinking: preset.6.to_string(),
    })
}

/// Return all known presets (for UI display).
pub fn all_presets() -> Vec<(&'static str, ProviderPreset)> {
    let names = [
        "DEEPSEEK",
        "OPENAI",
        "ANTHROPIC",
        "GEMINI",
        "MINIMAX",
        "MOONSHOT",
    ];
    names
        .iter()
        .filter_map(|n| {
            let env = format!("{}_API_KEY", n);
            provider_preset(&env).map(|p| (*n, p))
        })
        .collect()
}
