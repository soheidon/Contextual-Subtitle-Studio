use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Clone, Serialize)]
pub struct LogPayload {
    pub level: String,
    pub scope: String,
    pub message: String,
}

/// Take the first `max_chars` characters of a string safely (char boundary, not byte index).
/// Unicode-safe replacement for `&s[..s.len().min(N)]` which panics on multi-byte characters.
pub fn preview_chars(s: &str, max_chars: usize) -> String {
    let chars: Vec<char> = s.chars().take(max_chars + 1).collect();
    if chars.len() > max_chars {
        let mut out: String = chars[..max_chars].iter().collect();
        out.push_str("...");
        out
    } else {
        chars.iter().collect()
    }
}

/// Emit a log message to both stderr (PowerShell) and the frontend via Tauri event.
pub fn emit_log(app: &AppHandle, level: &str, scope: &str, message: &str) {
    eprintln!("[{}] [{}] {}", level.to_uppercase(), scope, message);
    let _ = app.emit(
        "app-log",
        LogPayload {
            level: level.to_string(),
            scope: scope.to_string(),
            message: message.to_string(),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_preview_chars_short() {
        let s = "Hello";
        let result = preview_chars(s, 10);
        assert_eq!(result, "Hello"); // no ellipsis when within limit
    }

    #[test]
    fn test_preview_chars_truncates_with_ellipsis() {
        let s = "abcdefghijklmnop"; // 16 chars
        let result = preview_chars(s, 10);
        assert_eq!(result.len(), 13); // 10 chars + "..."
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_preview_chars_japanese_no_panic() {
        let s = "楚喬は目を負傷し、李策が持ってきたHuotu Waterで治療を受けている。";
        // This used to panic with &s[..800] byte-index slicing
        let result = preview_chars(s, 800);
        assert!(result.contains("楚喬"), "Japanese chars should survive");
        // Should NOT end with "..." since s is shorter than 800 chars
        assert!(!result.ends_with("..."), "Should not truncate short input");
    }

    #[test]
    fn test_preview_chars_japanese_truncated() {
        let mut s = String::new();
        for _ in 0..50 {
            s.push_str("日本語テスト");
        }
        let result = preview_chars(&s, 10);
        assert_eq!(result.chars().count(), 13); // 10 chars + "..."
        assert!(result.ends_with("..."));
    }

    #[test]
    fn test_preview_chars_multibyte_boundary() {
        // Each char is 3 bytes, so byte index 800 would likely fall mid-char
        let s: String = std::iter::repeat('日').take(500).collect();
        let result = preview_chars(&s, 100);
        assert_eq!(result.chars().count(), 103); // 100 + "..."
        // Verify no replacement characters from broken UTF-8 boundaries
        assert!(!result.contains('\u{FFFD}'));
    }
}
