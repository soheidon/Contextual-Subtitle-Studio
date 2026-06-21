use super::config::*;
use serde_json::Value;

/// OpenAI-compatible LLM client.
pub struct LlmClient {
    config: ProviderConfig,
    client: reqwest::Client,
}

impl LlmClient {
    pub fn new(config: ProviderConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    /// Send a chat completion request and return the assistant's message content.
    pub async fn chat(
        &self,
        messages: &[ChatMessage],
        use_json_mode: bool,
    ) -> Result<String, String> {
        let url = format!(
            "{}/v1/chat/completions",
            self.config.base_url.trim_end_matches('/')
        );

        let thinking = self.config.thinking.as_deref().and_then(|t| match t {
            "enabled" => Some(ThinkingConfig {
                thinking_type: "enabled".to_string(),
            }),
            "auto" => Some(ThinkingConfig {
                thinking_type: "auto".to_string(),
            }),
            _ => None, // "disabled" or unknown → omit
        });

        let request_body = ChatCompletionRequest {
            model: self.config.model.clone(),
            messages: messages.to_vec(),
            temperature: Some(0.3),
            thinking,
            response_format: if use_json_mode {
                Some(ResponseFormat {
                    format_type: "json_object".to_string(),
                })
            } else {
                None
            },
        };

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.config.api_key))
            .header("Content-Type", "application/json")
            .json(&request_body)
            .send()
            .await
            .map_err(|e| format!("HTTP request failed: {}", e))?;

        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(format!("API error ({}): {}", status, body));
        }

        let completion: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse response: {}", e))?;

        let content = completion
            .choices
            .first()
            .ok_or("No choices in response")?
            .message
            .content
            .clone();

        let content = strip_thinking_tags(&content);
        Ok(content)
    }

    /// Send a chat completion request expecting a JSON response and return the parsed value.
    pub async fn chat_json(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Result<Value, String> {
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: user_prompt.to_string(),
            },
        ];
        let response = self.chat(&messages, true).await?;
        serde_json::from_str(&response)
            .map_err(|e| format!("Failed to parse JSON response: {} (raw: {})", e, &response[..response.len().min(200)]))
    }

    /// Test the connection by sending a minimal request.
    pub async fn test_connection(&self) -> Result<bool, String> {
        let messages = vec![ChatMessage {
            role: "user".to_string(),
            content: "Hello".to_string(),
        }];
        match self.chat(&messages, false).await {
            Ok(_) => Ok(true),
            Err(e) => Err(e),
        }
    }

    /// Translate a chunk of SRT entries and return the raw text response.
    pub async fn translate_chunk(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Result<String, String> {
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: user_prompt.to_string(),
            },
        ];
        self.chat(&messages, false).await
    }

    /// Request a JSON-structured translation response.
    pub async fn translate_chunk_json(
        &self,
        system_prompt: &str,
        user_prompt: &str,
    ) -> Result<Value, String> {
        let full_user_prompt = format!(
            "{}\n\n出力は以下のJSON形式で返してください：\n{{\"translations\": [{{\"index\": 番号, \"start\": \"開始時間\", \"end\": \"終了時間\", \"text\": \"日本語訳\"}}]}}",
            user_prompt
        );

        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: system_prompt.to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: full_user_prompt,
            },
        ];

        let response = self.chat(&messages, true).await?;
        serde_json::from_str(&response)
            .map_err(|e| format!("Failed to parse JSON response: {}", e))
    }
}

/// Remove thinking tags that some models (DeepSeek R1, MiniMax) include in their output.
fn strip_thinking_tags(content: &str) -> String {
    // Strip 思考 tags (DeepSeek R1)
    let content = regex::Regex::new(r"(?s)<思考>.*?</思考>")
        .unwrap()
        .replace_all(content, "");
    let content = regex::Regex::new(r"(?s)<thinking>.*?</thinking>")
        .unwrap()
        .replace_all(&content, "");
    // Strip 思考 tags (MiniMax)
    let content = regex::Regex::new(r"(?s)<thought>.*?</thought>")
        .unwrap()
        .replace_all(&content, "");
    content.trim().to_string()
}
