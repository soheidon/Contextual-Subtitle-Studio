use regex::Regex;
use super::SubtitleEntry;

/// Parse an SRT file content string into a Vec of SubtitleEntry.
pub fn parse_srt(content: &str) -> Result<Vec<SubtitleEntry>, String> {
    // Strip BOM if present
    let content = content.strip_prefix('\u{FEFF}').unwrap_or(content);

    let re = Regex::new(
        r"(?m)^(\d+)\s*\n(\d{2}:\d{2}:\d{2},\d{3})\s*-->\s*(\d{2}:\d{2}:\d{2},\d{3})\s*\n((?s).*?)\n\s*\n"
    )
    .map_err(|e| format!("Regex compilation error: {}", e))?;

    let mut entries = Vec::new();
    let mut seen_indices = std::collections::HashSet::new();

    for caps in re.captures_iter(content) {
        let index: u32 = caps[1]
            .parse()
            .map_err(|e| format!("Invalid index: {}", e))?;

        if seen_indices.contains(&index) {
            return Err(format!("Duplicate subtitle index: {}", index));
        }
        seen_indices.insert(index);

        let start = caps[2].to_string();
        let end = caps[3].to_string();
        let text = caps[4].trim().to_string();

        if text.is_empty() {
            return Err(format!("Empty text for subtitle index {}", index));
        }

        entries.push(SubtitleEntry {
            index,
            start,
            end,
            text,
        });
    }

    if entries.is_empty() {
        return Err("No valid SRT entries found".to_string());
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_srt() {
        let content = "\
1
00:00:01,000 --> 00:00:03,000
Hello, how are you?

2
00:00:03,500 --> 00:00:06,000
I'm fine, thank you.

";
        let entries = parse_srt(content).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].index, 1);
        assert_eq!(entries[0].start, "00:00:01,000");
        assert_eq!(entries[0].end, "00:00:03,000");
        assert_eq!(entries[0].text, "Hello, how are you?");
        assert_eq!(entries[1].index, 2);
        assert_eq!(entries[1].text, "I'm fine, thank you.");
    }

    #[test]
    fn test_multiline_text() {
        let content = "\
1
00:00:01,000 --> 00:00:04,000
Line one
Line two

2
00:00:05,000 --> 00:00:07,000
Single line

";
        let entries = parse_srt(content).unwrap();
        assert_eq!(entries[0].text, "Line one\nLine two");
        assert_eq!(entries[1].text, "Single line");
    }

    #[test]
    fn test_empty_input() {
        assert!(parse_srt("").is_err());
    }

    #[test]
    fn test_duplicate_index() {
        let content = "\
1
00:00:01,000 --> 00:00:03,000
First

1
00:00:04,000 --> 00:00:06,000
Second

";
        assert!(parse_srt(content).is_err());
    }

    #[test]
    fn test_parse_with_bom() {
        let content = "\u{FEFF}1
00:00:01,000 --> 00:00:03,000
Hello

";
        let entries = parse_srt(content).unwrap();
        assert_eq!(entries[0].text, "Hello");
    }
}
