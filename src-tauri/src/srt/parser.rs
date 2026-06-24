use super::SubtitleEntry;

/// Check whether `line` matches an SRT timecode: `HH:MM:SS,mmm --> HH:MM:SS,mmm`.
fn is_timecode_line(line: &str) -> bool {
    let line = line.trim();
    // Must have exactly two timestamp segments separated by "-->"
    let parts: Vec<&str> = line.split("-->").collect();
    if parts.len() != 2 {
        return false;
    }
    let left = parts[0].trim();
    let right = parts[1].trim();
    is_timestamp(left) && is_timestamp(right)
}

fn is_timestamp(s: &str) -> bool {
    // HH:MM:SS,mmm
    s.len() == 12
        && s.chars().nth(2) == Some(':')
        && s.chars().nth(5) == Some(':')
        && s.chars().nth(8) == Some(',')
        && s[..2].chars().all(|c| c.is_ascii_digit())
        && s[3..5].chars().all(|c| c.is_ascii_digit())
        && s[6..8].chars().all(|c| c.is_ascii_digit())
        && s[9..12].chars().all(|c| c.is_ascii_digit())
}

/// Parse `HH:MM:SS,mmm --> HH:MM:SS,mmm` into (start, end) strings.
fn parse_timecode_line(line: &str) -> Option<(String, String)> {
    if !is_timecode_line(line) {
        return None;
    }
    let parts: Vec<&str> = line.trim().split("-->").collect();
    if parts.len() != 2 {
        return None;
    }
    Some((parts[0].trim().to_string(), parts[1].trim().to_string()))
}

/// Check whether lines at `i` look like the start of the next subtitle block:
/// line i is parseable as u32 and line i+1 is a valid timecode.
fn looks_like_index_timecode_pair(lines: &[&str], i: usize) -> bool {
    if i + 1 >= lines.len() {
        return false;
    }
    lines[i].trim().parse::<u32>().is_ok() && is_timecode_line(lines[i + 1].trim())
}

/// Line-based SRT parser.
///
/// Reads lines sequentially: index → timecode → text until blank line (or
/// next index+timecode pair).  This avoids the regex backtracking bug that
/// caused empty-text entries to consume the next subtitle's header into their
/// text field.
///
/// Empty subtitles are preserved as `SubtitleEntry` with `text = ""`.
/// Filtering them out is the pipeline's responsibility, not the parser's.
pub fn parse_srt(content: &str) -> Result<Vec<SubtitleEntry>, String> {
    // Strip BOM if present
    let content = content.strip_prefix('\u{FEFF}').unwrap_or(content);

    // Normalize line endings
    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalized.lines().collect();

    let mut entries = Vec::new();
    let mut seen_indices = std::collections::HashSet::new();
    let mut i: usize = 0;

    while i < lines.len() {
        // Skip blank lines between blocks
        while i < lines.len() && lines[i].trim().is_empty() {
            i += 1;
        }
        if i >= lines.len() {
            break;
        }

        // Parse index line
        let index_line = lines[i].trim();
        let index: u32 = index_line
            .parse()
            .map_err(|_| format!("Invalid SRT index line: \"{}\"", index_line))?;

        if seen_indices.contains(&index) {
            return Err(format!("Duplicate subtitle index: {}", index));
        }
        seen_indices.insert(index);
        i += 1;

        // Parse timecode line
        if i >= lines.len() {
            return Err(format!("Missing timecode for subtitle {}", index));
        }

        let time_line = lines[i].trim();
        let (start, end) = parse_timecode_line(time_line)
            .ok_or_else(|| format!("Invalid timecode for subtitle {}: \"{}\"", index, time_line))?;
        i += 1;

        // Collect text lines until a blank line or the start of the next block
        let mut text_lines: Vec<&str> = Vec::new();

        while i < lines.len() {
            let line = lines[i];

            // Blank line → end of this subtitle block
            if line.trim().is_empty() {
                i += 1;
                break;
            }

            // Defense-in-depth: if the current line looks like the next index
            // and the following line is a timecode, treat it as the start of
            // the next block even without a preceding blank line.
            if looks_like_index_timecode_pair(&lines, i) {
                break;
            }

            text_lines.push(line);
            i += 1;
        }

        let text = text_lines.join("\n").trim().to_string();

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

    // -- empty subtitle tests --

    #[test]
    fn test_parse_empty_subtitle_does_not_consume_next_entry() {
        // Entry 40 has blank text. Entry 41 must be parsed separately.
        let content = "\
40
00:06:12,040 --> 00:06:23,940

41
00:07:51,840 --> 00:07:55,600
大雍、卞唐、西盟。

";
        let entries = parse_srt(content).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].index, 40);
        assert_eq!(entries[0].start, "00:06:12,040");
        assert_eq!(entries[0].end, "00:06:23,940");
        assert_eq!(entries[0].text, "");

        assert_eq!(entries[1].index, 41);
        assert_eq!(entries[1].start, "00:07:51,840");
        assert_eq!(entries[1].end, "00:07:55,600");
        assert_eq!(entries[1].text, "大雍、卞唐、西盟。");
    }

    #[test]
    fn test_parse_empty_subtitle_between_normal_entries() {
        let content = "\
1
00:00:01,000 --> 00:00:02,000
Hello

2
00:00:03,000 --> 00:00:04,000

3
00:00:05,000 --> 00:00:06,000
World

";
        let entries = parse_srt(content).unwrap();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].text, "Hello");
        assert_eq!(entries[1].text, "");
        assert_eq!(entries[2].text, "World");
    }

    #[test]
    fn test_parse_multiple_empty_subtitles() {
        let content = "\
1
00:00:01,000 --> 00:00:02,000
A

2
00:00:03,000 --> 00:00:04,000

3
00:00:05,000 --> 00:00:06,000

4
00:00:07,000 --> 00:00:08,000
B

";
        let entries = parse_srt(content).unwrap();
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0].text, "A");
        assert_eq!(entries[1].text, "");
        assert_eq!(entries[2].text, "");
        assert_eq!(entries[3].text, "B");
    }

    #[test]
    fn test_parse_last_empty_subtitle() {
        let content = "\
1
00:00:01,000 --> 00:00:02,000
Hello

2
00:00:03,000 --> 00:00:04,000

";
        let entries = parse_srt(content).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].text, "Hello");
        assert_eq!(entries[1].text, "");
    }

    #[test]
    fn test_parse_does_not_embed_next_index_or_timecode_in_text() {
        // Regression test: empty subtitles must not consume the next block's
        // header into their text field.
        let content = "\
440
00:39:10,023 --> 00:39:12,873

441
00:39:20,240 --> 00:39:21,780
But he won't survive.

";
        let entries = parse_srt(content).unwrap();
        // Each entry's text must not contain the next subtitle's index or timecode
        for entry in &entries {
            assert!(
                !entry.text.contains('\n'),
                "Entry {} text contains embedded newline: {:?}",
                entry.index,
                entry.text
            );
            assert!(
                !entry.text.contains(" --> "),
                "Entry {} text contains timecode arrow: {:?}",
                entry.index,
                entry.text
            );
            // Should not be a bare number (empty text is fine)
            assert!(
                entry.text.is_empty()
                    || !(entry.text.len() <= 4 && entry.text.chars().all(|c| c.is_ascii_digit())),
                "Entry {} text is a bare number: {:?}",
                entry.index,
                entry.text
            );
        }
        assert_eq!(entries[0].index, 440);
        assert_eq!(entries[0].text, "");
        assert_eq!(entries[1].index, 441);
        assert_eq!(entries[1].text, "But he won't survive.");
    }

    #[test]
    fn test_looks_like_index_timecode_pair_true() {
        let lines: Vec<&str> = vec!["42", "00:01:00,000 --> 00:01:02,000", "some text"];
        assert!(looks_like_index_timecode_pair(&lines, 0));
    }

    #[test]
    fn test_looks_like_index_timecode_pair_not_a_number() {
        let lines: Vec<&str> = vec!["not a number", "00:01:00,000 --> 00:01:02,000"];
        assert!(!looks_like_index_timecode_pair(&lines, 0));
    }

    #[test]
    fn test_looks_like_index_timecode_pair_not_a_timecode() {
        let lines: Vec<&str> = vec!["42", "not a timecode"];
        assert!(!looks_like_index_timecode_pair(&lines, 0));
    }

    #[test]
    fn test_looks_like_index_timecode_pair_not_enough_lines() {
        let lines: Vec<&str> = vec!["42"];
        assert!(!looks_like_index_timecode_pair(&lines, 0));
    }
}
