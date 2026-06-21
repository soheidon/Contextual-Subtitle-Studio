use crate::character_dict::{CharacterDict, PastedEntry};
use serde::{Deserialize, Serialize};

/// Metadata about the drama being processed.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DramaMetadata {
    pub drama_title: Option<String>,
    pub douban_url: Option<String>,
    pub tmdb_url: Option<String>,
    pub updated_at: Option<String>,
}

/// All drama-related data bundled for save/load.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DramaInfoBundle {
    pub metadata: Option<DramaMetadata>,
    pub synopsis_douban: Option<String>,
    pub synopsis_imdb: Option<String>,
    pub cast_douban: Option<Vec<PastedEntry>>,
    pub cast_imdb: Option<Vec<PastedEntry>>,
    pub character_dict: Option<CharacterDict>,
}

/// Save all drama info to `<dir>/drama_info/`.
/// None fields are skipped (partial updates supported).
#[tauri::command]
pub fn save_drama_info(dir: String, bundle: DramaInfoBundle) -> Result<(), String> {
    let base = std::path::Path::new(&dir).join("drama_info");
    std::fs::create_dir_all(&base)
        .map_err(|e| format!("Failed to create drama_info dir: {}", e))?;

    if let Some(ref metadata) = bundle.metadata {
        write_json(&base.join("metadata.json"), metadata)?;
    }
    if let Some(ref text) = bundle.synopsis_douban {
        write_text(&base.join("synopsis_douban.txt"), text)?;
    }
    if let Some(ref text) = bundle.synopsis_imdb {
        write_text(&base.join("synopsis_imdb.txt"), text)?;
    }
    if let Some(ref entries) = bundle.cast_douban {
        write_json(&base.join("cast_douban.json"), entries)?;
    }
    if let Some(ref entries) = bundle.cast_imdb {
        write_json(&base.join("cast_imdb.json"), entries)?;
    }
    if let Some(ref dict) = bundle.character_dict {
        write_json(&base.join("character_dict.json"), dict)?;
    }

    Ok(())
}

/// Load all drama info from `<dir>/drama_info/`.
/// Missing files result in None fields.
#[tauri::command]
pub fn load_drama_info(dir: String) -> Result<DramaInfoBundle, String> {
    let base = std::path::Path::new(&dir).join("drama_info");
    if !base.exists() {
        return Ok(DramaInfoBundle::default());
    }

    Ok(DramaInfoBundle {
        metadata: read_json(&base.join("metadata.json")),
        synopsis_douban: read_text(&base.join("synopsis_douban.txt")),
        synopsis_imdb: read_text(&base.join("synopsis_imdb.txt")),
        cast_douban: read_json(&base.join("cast_douban.json")),
        cast_imdb: read_json(&base.join("cast_imdb.json")),
        character_dict: read_json(&base.join("character_dict.json")),
    })
}

fn write_json<T: Serialize>(path: &std::path::Path, value: &T) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| format!("Failed to serialize {}: {}", path.display(), e))?;
    std::fs::write(path, json)
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))
}

fn write_text(path: &std::path::Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text)
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))
}

fn read_json<T: for<'de> Deserialize<'de> + Default>(path: &std::path::Path) -> Option<T> {
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

fn read_text(path: &std::path::Path) -> Option<String> {
    if !path.exists() {
        return None;
    }
    std::fs::read_to_string(path).ok()
}
