use super::synopsis::SynopsisSummary;
use crate::character_dict::{CharacterDict, MergedCastEntry, PastedEntry};
use serde::{Deserialize, Serialize};

/// Metadata about the drama being processed.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DramaMetadata {
    pub drama_title: Option<String>,
    pub douban_url: Option<String>,
    pub tmdb_url: Option<String>,
    pub search_title_zh: Option<String>,
    pub search_title_en: Option<String>,
    pub search_year: Option<String>,
    pub updated_at: Option<String>,
}

/// All drama-related data bundled for save/load.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DramaInfoBundle {
    pub metadata: Option<DramaMetadata>,
    pub synopsis_douban: Option<String>,
    #[serde(alias = "synopsis_imdb")]
    pub synopsis_tmdb: Option<String>,
    pub cast_douban: Option<Vec<PastedEntry>>,
    #[serde(alias = "cast_imdb")]
    pub cast_tmdb: Option<Vec<PastedEntry>>,
    pub cast_mdl: Option<Vec<PastedEntry>>,
    pub character_dict: Option<CharacterDict>,
    pub synopsis_summary: Option<SynopsisSummary>,
    pub merged_cast: Option<Vec<MergedCastEntry>>,
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
    if let Some(ref text) = bundle.synopsis_tmdb {
        write_text(&base.join("synopsis_tmdb.txt"), text)?;
        let _ = std::fs::remove_file(&base.join("synopsis_imdb.txt")); // 旧ファイル削除
    }
    if let Some(ref entries) = bundle.cast_douban {
        write_json(&base.join("cast_douban.json"), entries)?;
    }
    if let Some(ref entries) = bundle.cast_tmdb {
        write_json(&base.join("cast_tmdb.json"), entries)?;
        let _ = std::fs::remove_file(&base.join("cast_imdb.json")); // 旧ファイル削除
    }
    if let Some(ref entries) = bundle.cast_mdl {
        write_json(&base.join("cast_mdl.json"), entries)?;
    }
    if let Some(ref dict) = bundle.character_dict {
        write_json(&base.join("character_dict.json"), dict)?;
    }
    if let Some(ref summary) = bundle.synopsis_summary {
        write_json(&base.join("synopsis_summary.json"), summary)?;
    }
    if let Some(ref cast) = bundle.merged_cast {
        write_json(&base.join("merged_cast.json"), cast)?;
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
        synopsis_tmdb: read_text(&base.join("synopsis_tmdb.txt"))
            .or_else(|| read_text(&base.join("synopsis_imdb.txt"))), // 旧ファイル名互換
        cast_douban: read_json(&base.join("cast_douban.json")),
        cast_tmdb: read_json(&base.join("cast_tmdb.json"))
            .or_else(|| read_json(&base.join("cast_imdb.json"))), // 旧ファイル名互換
        cast_mdl: read_json(&base.join("cast_mdl.json")),
        character_dict: read_json(&base.join("character_dict.json")),
        synopsis_summary: read_json(&base.join("synopsis_summary.json")),
        merged_cast: read_json(&base.join("merged_cast.json")),
    })
}

fn write_json<T: Serialize>(path: &std::path::Path, value: &T) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|e| format!("Failed to serialize {}: {}", path.display(), e))?;
    std::fs::write(path, json).map_err(|e| format!("Failed to write {}: {}", path.display(), e))
}

fn write_text(path: &std::path::Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|e| format!("Failed to write {}: {}", path.display(), e))
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
