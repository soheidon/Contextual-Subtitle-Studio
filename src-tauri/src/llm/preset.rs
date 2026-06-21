use serde::{Deserialize, Serialize};

/// A preset mapping an env var prefix to (base_url, model).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderPreset {
    pub provider: String,
    pub base_url: String,
    pub model: String,
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
        "DEEPSEEK" => ("DeepSeek", "https://api.deepseek.com", "deepseek-v4-flash"),
        "OPENAI" => ("OpenAI", "https://api.openai.com", "gpt-4o-mini"),
        "ANTHROPIC" | "CLAUDE" => ("Anthropic", "https://api.anthropic.com", "claude-3-5-sonnet-latest"),
        "GEMINI" | "GOOGLE" => ("Gemini", "https://generativelanguage.googleapis.com/v1beta/openai", "gemini-2.0-flash"),
        "MINIMAX" => ("MiniMax", "https://api.MiniMax.chat", "MiniMax-chat"),
        "MOONSHOT" | "KIMI" => ("Kimi / Moonshot", "https://api.moonshot.cn", "moonshot-v1-8k"),
        _ => return None,
    };

    Some(ProviderPreset {
        provider: preset.0.to_string(),
        base_url: preset.1.to_string(),
        model: preset.2.to_string(),
    })
}

/// Return all known presets (for UI display).
pub fn all_presets() -> Vec<(&'static str, ProviderPreset)> {
    let names = [
        "DEEPSEEK", "OPENAI", "ANTHROPIC", "GEMINI", "MINIMAX", "MOONSHOT",
    ];
    names
        .iter()
        .filter_map(|n| {
            let env = format!("{}_API_KEY", n);
            provider_preset(&env).map(|p| (*n, p))
        })
        .collect()
}
