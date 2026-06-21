use super::SubtitleEntry;

/// Format a Vec of SubtitleEntry back into an SRT string.
pub fn write_srt(entries: &[SubtitleEntry]) -> String {
    let mut output = String::new();
    for (i, entry) in entries.iter().enumerate() {
        if i > 0 {
            output.push('\n');
        }
        output.push_str(&format!(
            "{}\n{} --> {}\n{}\n",
            entry.index, entry.start, entry.end, entry.text
        ));
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_roundtrip() {
        let entries = vec![
            SubtitleEntry {
                index: 1,
                start: "00:00:01,000".into(),
                end: "00:00:03,000".into(),
                text: "Hello".into(),
            },
            SubtitleEntry {
                index: 2,
                start: "00:00:04,000".into(),
                end: "00:00:06,000".into(),
                text: "World".into(),
            },
        ];
        let srt = write_srt(&entries);
        assert!(srt.contains("1\n00:00:01,000 --> 00:00:03,000\nHello\n"));
        assert!(srt.contains("2\n00:00:04,000 --> 00:00:06,000\nWorld\n"));
    }
}
