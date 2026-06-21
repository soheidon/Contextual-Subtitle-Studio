use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Character {
    pub id: String,
    pub english_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chinese_name: Option<String>,
    pub japanese_name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gender: Option<String>,
    #[serde(default = "default_register")]
    pub default_register: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speech_style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

fn default_register() -> String {
    "neutral".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CharacterCsvRow {
    id: String,
    english_name: String,
    chinese_name: String,
    japanese_name: String,
    aliases: String,
    role: String,
    status: String,
    gender: String,
    default_register: String,
    speech_style: String,
    notes: String,
}

/// Load characters from a JSON file path. Expects a JSON object with a "characters" key.
pub fn load_characters_json(json_str: &str) -> Result<Vec<Character>, String> {
    #[derive(Deserialize)]
    struct Wrapper {
        characters: Vec<Character>,
    }

    let wrapper: Wrapper =
        serde_json::from_str(json_str).map_err(|e| format!("Invalid JSON: {}", e))?;
    Ok(wrapper.characters)
}

/// Load characters from a CSV file path.
/// Expected columns: id, english_name, chinese_name, japanese_name, aliases, role, status, gender, default_register, speech_style, notes
pub fn load_characters_csv(csv_str: &str) -> Result<Vec<Character>, String> {
    let mut reader = csv::Reader::from_reader(csv_str.as_bytes());
    let mut characters = Vec::new();

    for result in reader.deserialize::<CharacterCsvRow>() {
        let row = result.map_err(|e| format!("CSV parse error: {}", e))?;
        characters.push(Character {
            id: row.id,
            english_name: row.english_name,
            chinese_name: optional_string(&row.chinese_name),
            japanese_name: row.japanese_name,
            aliases: parse_semicolon_list(&row.aliases),
            role: optional_string(&row.role),
            status: optional_string(&row.status),
            gender: optional_string(&row.gender),
            default_register: if row.default_register.is_empty() {
                "neutral".to_string()
            } else {
                row.default_register
            },
            speech_style: optional_string(&row.speech_style),
            notes: optional_string(&row.notes),
        });
    }

    if characters.is_empty() {
        return Err("No character entries found in CSV".to_string());
    }

    Ok(characters)
}

/// Save characters to JSON string.
pub fn save_characters_json(characters: &[Character]) -> Result<String, String> {
    #[derive(Serialize)]
    struct Wrapper {
        characters: Vec<Character>,
    }

    let wrapper = Wrapper {
        characters: characters.to_vec(),
    };
    serde_json::to_string_pretty(&wrapper).map_err(|e| format!("JSON serialize error: {}", e))
}

fn optional_string(s: &str) -> Option<String> {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn parse_semicolon_list(s: &str) -> Vec<String> {
    s.split(';')
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_characters_json() {
        let json = r#"{
            "characters": [
                {
                    "id": "chu_qiao",
                    "english_name": "Chu Qiao",
                    "chinese_name": "楚乔",
                    "japanese_name": "楚喬",
                    "aliases": ["Qiao", "Xing'er"],
                    "role": "主人公",
                    "status": "奴籍少女",
                    "gender": "female",
                    "default_register": "neutral",
                    "speech_style": "中性的で簡潔な字幕調",
                    "notes": "過剰な女言葉は避ける"
                }
            ]
        }"#;

        let chars = load_characters_json(json).unwrap();
        assert_eq!(chars.len(), 1);
        assert_eq!(chars[0].id, "chu_qiao");
        assert_eq!(chars[0].english_name, "Chu Qiao");
        assert_eq!(chars[0].japanese_name, "楚喬");
        assert_eq!(chars[0].aliases, vec!["Qiao", "Xing'er"]);
        assert_eq!(chars[0].default_register, "neutral");
    }

    #[test]
    fn test_load_characters_csv() {
        let csv = "id,english_name,chinese_name,japanese_name,aliases,role,status,gender,default_register,speech_style,notes\nchu_qiao,Chu Qiao,楚乔,楚喬,Qiao;Xing'er,主人公,奴籍少女,female,neutral,中性的で簡潔な字幕調,過剰な女言葉は避ける\n";

        let chars = load_characters_csv(csv).unwrap();
        assert_eq!(chars.len(), 1);
        assert_eq!(chars[0].id, "chu_qiao");
        assert_eq!(chars[0].english_name, "Chu Qiao");
        assert_eq!(chars[0].japanese_name, "楚喬");
        assert_eq!(chars[0].aliases, vec!["Qiao", "Xing'er"]);
    }

    #[test]
    fn test_save_characters_json() {
        let chars = vec![Character {
            id: "test".into(),
            english_name: "Test".into(),
            japanese_name: "テスト".into(),
            aliases: vec![],
            chinese_name: None,
            role: None,
            status: None,
            gender: None,
            default_register: "neutral".into(),
            speech_style: None,
            notes: None,
        }];
        let json = save_characters_json(&chars).unwrap();
        assert!(json.contains("test"));
        assert!(json.contains("Test"));
        assert!(json.contains("characters"));
    }
}
