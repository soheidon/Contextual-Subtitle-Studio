use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub provider: String,
    #[serde(rename = "base_url")]
    pub base_url: String,
    #[serde(rename = "api_key")]
    pub api_key: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationConfig {
    #[serde(rename = "max_chars_per_line")]
    pub max_chars_per_line: u32,
    #[serde(rename = "max_lines_per_subtitle")]
    pub max_lines_per_subtitle: u32,
    pub style: String,
    #[serde(rename = "avoid_gendered_speech")]
    pub avoid_gendered_speech: bool,
}

impl Default for TranslationConfig {
    fn default() -> Self {
        Self {
            max_chars_per_line: 24,
            max_lines_per_subtitle: 2,
            style: "neutral_subtitle".to_string(),
            avoid_gendered_speech: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationProgress {
    pub progress: f64,
    pub current_chunk: u32,
    pub total_chunks: u32,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(rename = "response_format")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormat>,
}

#[derive(Debug, Serialize)]
pub struct ResponseFormat {
    #[serde(rename = "type")]
    pub format_type: String,
}

#[derive(Debug, Deserialize)]
pub struct ChatCompletionResponse {
    pub choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
pub struct Choice {
    pub message: ChoiceMessage,
}

#[derive(Debug, Deserialize)]
pub struct ChoiceMessage {
    pub content: String,
}
