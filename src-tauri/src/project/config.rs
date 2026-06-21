use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectConfig {
    pub project: ProjectInfo,
    pub translation: TranslationSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectInfo {
    pub name: String,
    pub title: Option<String>,
    pub episode: Option<u32>,
    pub source_language: String,
    pub target_language: String,
    pub base_dir: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationSettings {
    pub style: String,
    pub avoid_gendered_speech: bool,
    pub preserve_srt_timing: bool,
    pub max_chars_per_line: u32,
    pub max_lines_per_subtitle: u32,
}

impl Default for ProjectConfig {
    fn default() -> Self {
        Self {
            project: ProjectInfo {
                name: "New Project".into(),
                title: None,
                episode: None,
                source_language: "en".into(),
                target_language: "ja".into(),
                base_dir: ".".into(),
            },
            translation: TranslationSettings {
                style: "neutral_subtitle".into(),
                avoid_gendered_speech: true,
                preserve_srt_timing: true,
                max_chars_per_line: 24,
                max_lines_per_subtitle: 2,
            },
        }
    }
}

impl ProjectConfig {
    pub fn save_to_file(&self, path: &str) -> Result<(), String> {
        let yaml = serde_yaml::to_string(self).map_err(|e| format!("YAML error: {}", e))?;
        std::fs::write(path, yaml).map_err(|e| format!("IO error: {}", e))?;
        Ok(())
    }

    pub fn load_from_file(path: &str) -> Result<Self, String> {
        let content = std::fs::read_to_string(path).map_err(|e| format!("IO error: {}", e))?;
        serde_yaml::from_str(&content).map_err(|e| format!("YAML parse error: {}", e))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub name: String,
    pub base_dir: String,
    pub is_open: bool,
}
