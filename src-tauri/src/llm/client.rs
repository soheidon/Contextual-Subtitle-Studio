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
        let trimmed = self.config.base_url.trim_end_matches('/');
        let url = if trimmed.ends_with("/v1") {
            format!("{}/chat/completions", trimmed)
        } else {
            format!("{}/v1/chat/completions", trimmed)
        };

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
        let cleaned = extract_json(&response);
        serde_json::from_str(&cleaned)
            .map_err(|e| format!("Failed to parse JSON response: {} (raw: {})", e, crate::log::preview_chars(&response, 200)))
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
        let cleaned = extract_json(&response);
        serde_json::from_str(&cleaned)
            .map_err(|e| format!("Failed to parse JSON response: {}", e))
    }
}

use std::sync::LazyLock;

// Pre-compiled patterns for thinking tag stripping (DeepSeek R1, MiniMax).
// Static patterns — compile once, never panic at call site.
static RE_THINK_ZH: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?s)<思考>.*?</思考>").expect("valid regex"));
static RE_THINK_EN: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?s)<thinking>.*?</thinking>").expect("valid regex"));
static RE_THOUGHT: LazyLock<regex::Regex> =
    LazyLock::new(|| regex::Regex::new(r"(?s)<thought>.*?</thought>").expect("valid regex"));

/// Remove thinking tags that some models (DeepSeek R1, MiniMax) include in their output.
fn strip_thinking_tags(content: &str) -> String {
    let content = RE_THINK_ZH.replace_all(content, "");
    let content = RE_THINK_EN.replace_all(&content, "");
    let content = RE_THOUGHT.replace_all(&content, "");
    content.trim().to_string()
}

/// Extract a JSON object or array from LLM output that may contain markdown code fences,
/// preamble text, or trailing commentary.
///
/// Handles:
///   - ` ```json\n{...}\n``` ` (at start, or after preamble)
///   - ` ```\n{...}\n``` `
///   - `{...}` (bare JSON)
///   - Preamble like "Here is the JSON:\n{...}" — skips to first `{` or `[`
pub fn extract_json(raw: &str) -> &str {
    let trimmed = raw.trim();

    // 1) Try to extract content inside a code fence (may appear anywhere, e.g. after preamble)
    //    Pattern: optional-lang-tag \n content \n ```
    if let Some(fence_start) = trimmed.find("```") {
        let after_open = &trimmed[fence_start + 3..];
        // Skip optional language tag on the same line as opening ```
        let body_start = after_open
            .find('\n')
            .map(|i| i + 1)
            .unwrap_or(0);
        let body = &after_open[body_start..];
        // Find the closing ```
        if let Some(fence_end) = body.find("```") {
            let inner = body[..fence_end].trim_end();
            // Verify the inner content actually contains JSON before returning it
            if inner.contains('{') || inner.contains('[') {
                let json_start = inner.find(|c: char| c == '{' || c == '[');
                if let Some(start) = json_start {
                    return &inner[start..];
                }
            }
        }
    }

    // 2) No code fence — skip any preamble before the first { or [
    let json_start = trimmed.find(|c: char| c == '{' || c == '[');
    let Some(start) = json_start else {
        return trimmed; // no JSON bracket found — return as-is, let serde fail with clear message
    };
    &trimmed[start..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_json_bare_object() {
        let raw = r#"{"key": "value"}"#;
        let result = extract_json(raw);
        assert_eq!(result, raw);
        assert!(serde_json::from_str::<serde_json::Value>(result).is_ok());
    }

    #[test]
    fn extract_json_bare_array() {
        let raw = r#"[1, 2, 3]"#;
        assert_eq!(extract_json(raw), raw);
    }

    #[test]
    fn extract_json_code_fence_json() {
        let raw = "```json\n{\"key\": \"value\"}\n```";
        let result = extract_json(raw);
        assert_eq!(result, "{\"key\": \"value\"}");
        assert!(serde_json::from_str::<serde_json::Value>(result).is_ok());
    }

    #[test]
    fn extract_json_code_fence_no_lang() {
        let raw = "```\n{\"key\": 42}\n```";
        let result = extract_json(raw);
        assert_eq!(result, "{\"key\": 42}");
    }

    #[test]
    fn extract_json_preamble_text() {
        let raw = "Here is the JSON you requested:\n{\"scenes\": []}";
        let result = extract_json(raw);
        assert_eq!(result, "{\"scenes\": []}");
    }

    #[test]
    fn extract_json_code_fence_with_preamble() {
        let raw = "Sure! Here it is:\n```json\n{\"ok\": true}\n```";
        let result = extract_json(raw);
        assert_eq!(result, "{\"ok\": true}");
    }

    #[test]
    fn extract_json_nested_braces() {
        let raw = "```json\n{\"a\": {\"b\": [1, 2]}, \"c\": \"d\"}\n```";
        let result = extract_json(raw);
        assert_eq!(result, "{\"a\": {\"b\": [1, 2]}, \"c\": \"d\"}");
        let v: serde_json::Value = serde_json::from_str(result).unwrap();
        assert_eq!(v["a"]["b"][0], 1);
    }

    #[test]
    fn extract_json_no_bracket_returns_as_is() {
        let raw = "no json here";
        assert_eq!(extract_json(raw), raw);
    }
}
