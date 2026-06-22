use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GlossaryEntry {
    pub source: String,
    pub target: String,
    #[serde(rename = "type")]
    pub entry_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence_urls: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GlossaryCsvRow {
    source: String,
    target: String,
    #[serde(rename = "type")]
    entry_type: String,
    notes: String,
}

/// Load glossary entries from a JSON string. Expects a JSON object with a "glossary" key.
pub fn load_glossary_json(json_str: &str) -> Result<Vec<GlossaryEntry>, String> {
    #[derive(Deserialize)]
    struct Wrapper {
        glossary: Vec<GlossaryEntry>,
    }

    let wrapper: Wrapper =
        serde_json::from_str(json_str).map_err(|e| format!("Invalid JSON: {}", e))?;
    Ok(wrapper.glossary)
}

/// Load glossary entries from a CSV string.
/// Expected columns: source, target, type, notes
pub fn load_glossary_csv(csv_str: &str) -> Result<Vec<GlossaryEntry>, String> {
    let mut reader = csv::Reader::from_reader(csv_str.as_bytes());
    let mut entries = Vec::new();

    for result in reader.deserialize::<GlossaryCsvRow>() {
        let row = result.map_err(|e| format!("CSV parse error: {}", e))?;
        entries.push(GlossaryEntry {
            source: row.source,
            target: row.target,
            entry_type: row.entry_type,
            aliases: Vec::new(),
            notes: if row.notes.trim().is_empty() {
                None
            } else {
                Some(row.notes.trim().to_string())
            },
            status: None,
            confidence: None,
            evidence_urls: None,
        });
    }

    if entries.is_empty() {
        return Err("No glossary entries found in CSV".to_string());
    }

    Ok(entries)
}

/// Save glossary entries to JSON string.
pub fn save_glossary_json(entries: &[GlossaryEntry]) -> Result<String, String> {
    #[derive(Serialize)]
    struct Wrapper {
        glossary: Vec<GlossaryEntry>,
    }

    let wrapper = Wrapper {
        glossary: entries.to_vec(),
    };
    serde_json::to_string_pretty(&wrapper).map_err(|e| format!("JSON serialize error: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_glossary_json() {
        let json = r#"{
            "glossary": [
                {
                    "source": "His Majesty",
                    "target": "陛下",
                    "type": "title",
                    "notes": "皇帝への呼称"
                },
                {
                    "source": "Yanbei",
                    "target": "燕北",
                    "type": "place"
                }
            ]
        }"#;

        let entries = load_glossary_json(json).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].source, "His Majesty");
        assert_eq!(entries[0].target, "陛下");
        assert_eq!(entries[0].entry_type, "title");
        assert_eq!(entries[1].source, "Yanbei");
        assert_eq!(entries[1].notes, None);
    }

    #[test]
    fn test_load_glossary_csv() {
        let csv = "source,target,type,notes\nHis Majesty,陛下,title,皇帝への呼称\nYanbei,燕北,place,\n";

        let entries = load_glossary_csv(csv).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].source, "His Majesty");
        assert_eq!(entries[0].target, "陛下");
        assert_eq!(entries[0].entry_type, "title");
        assert_eq!(entries[1].notes, None);
    }

    #[test]
    fn test_empty_glossary_json() {
        let json = r#"{"glossary": []}"#;
        let entries = load_glossary_json(json).unwrap();
        assert!(entries.is_empty());
    }
}
