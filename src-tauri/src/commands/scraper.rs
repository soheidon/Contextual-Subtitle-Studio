use crate::character_dict::{self, CharacterDict, PastedEntry, QualityReport};
use crate::dictionary::characters::Character;
use crate::merge::{self, MergedCharacter};
use crate::scraper::{self, ScrapeResult, ScrapeSource};
use tauri::State;

use crate::commands::project::AppState;

// ---------------------------------------------------------------------------
// scrape_url
// ---------------------------------------------------------------------------

/// Fetch and parse a single URL. Saves raw HTML to disk alongside the result.
#[tauri::command]
pub async fn scrape_url(
    _state: State<'_, AppState>,
    url: String,
    source: ScrapeSource,
) -> Result<ScrapeResult, String> {
    let result = match source {
        ScrapeSource::MyDramaList => scraper::mydramalist::scrape_mydramalist(&url).await?,
        ScrapeSource::TvMao => scraper::tvmao::scrape_tvmao(&url).await?,
        ScrapeSource::Douban => scraper::douban::scrape_douban(&url).await?,
        ScrapeSource::Tmdb => scraper::tmdb::scrape_tmdb_from_url(&url).await?,
        ScrapeSource::Other(_) => {
            return Err("Custom source scraping is not yet implemented.".to_string());
        }
    };

    Ok(result)
}

// ---------------------------------------------------------------------------
// scrape_all
// ---------------------------------------------------------------------------

/// Scrape all three sources concurrently. Returns (mdl, cn_cast, cn_meta).
#[tauri::command]
pub async fn scrape_all(
    _state: State<'_, AppState>,
    imdb_url: Option<String>,
    tvmao_url: Option<String>,
    douban_url: Option<String>,
) -> Result<(Option<ScrapeResult>, Option<ScrapeResult>, Option<ScrapeResult>), String> {
    let imdb_fut = async {
        if let Some(url) = &imdb_url {
            match scraper::tmdb::scrape_tmdb_from_url(url).await {
                Ok(r) => Some(r),
                Err(e) => {
                    eprintln!("TMDb scrape failed: {}", e);
                    None
                }
            }
        } else {
            None
        }
    };

    let tvmao_fut = async {
        if let Some(url) = &tvmao_url {
            match scraper::tvmao::scrape_tvmao(url).await {
                Ok(r) => Some(r),
                Err(e) => {
                    eprintln!("TVMao scrape failed: {}", e);
                    None
                }
            }
        } else {
            None
        }
    };

    let douban_fut = async {
        if let Some(url) = &douban_url {
            match scraper::douban::scrape_douban(url).await {
                Ok(r) => Some(r),
                Err(e) => {
                    eprintln!("Douban scrape failed: {}", e);
                    None
                }
            }
        } else {
            None
        }
    };

    let (imdb, tvmao, douban) = tokio::join!(imdb_fut, tvmao_fut, douban_fut);

    Ok((imdb, tvmao, douban))
}

// ---------------------------------------------------------------------------
// merge_characters
// ---------------------------------------------------------------------------

/// Merge scraped results from multiple sources into a unified character list.
#[tauri::command]
pub fn merge_characters(
    _state: State<'_, AppState>,
    mdl: Option<ScrapeResult>,
    cn_cast: Option<ScrapeResult>,
    cn_meta: Option<ScrapeResult>,
) -> Vec<MergedCharacter> {
    merge::merge_from_results(&mdl, &cn_cast, &cn_meta)
}

// ---------------------------------------------------------------------------
// save_scrape_result / load_scrape_result
// ---------------------------------------------------------------------------

/// Save a single ScrapeResult as JSON to the metadata/extracted/ directory.
#[tauri::command]
pub fn save_scrape_result(dir: String, result: ScrapeResult) -> Result<String, String> {
    let source_name = match &result.source {
        ScrapeSource::MyDramaList => "mydramalist",
        ScrapeSource::TvMao => "tvmao",
        ScrapeSource::Douban => "douban",
        ScrapeSource::Tmdb => "tmdb",
        ScrapeSource::Other(s) => s.as_str(),
    };

    let extracted_dir = std::path::Path::new(&dir).join("metadata").join("extracted");
    std::fs::create_dir_all(&extracted_dir)
        .map_err(|e| format!("Failed to create extracted dir: {}", e))?;

    let file_path = extracted_dir.join(format!("{}.json", source_name));
    let json = serde_json::to_string_pretty(&result)
        .map_err(|e| format!("Failed to serialize scrape result: {}", e))?;
    std::fs::write(&file_path, &json)
        .map_err(|e| format!("Failed to write {}: {}", file_path.display(), e))?;

    Ok(file_path.to_string_lossy().to_string())
}

/// Load a ScrapeResult from a JSON file.
#[tauri::command]
pub fn load_scrape_result(path: String) -> Result<ScrapeResult, String> {
    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("Failed to read {}: {}", path, e))?;
    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to deserialize {}: {}", path, e))
}

// ---------------------------------------------------------------------------
// save_merged_characters / load_merged_characters
// ---------------------------------------------------------------------------

/// Save merged characters to dictionaries/characters.json.
#[tauri::command]
pub fn save_merged_characters(
    dir: String,
    characters: Vec<MergedCharacter>,
) -> Result<String, String> {
    let dict_dir = std::path::Path::new(&dir).join("dictionaries");
    std::fs::create_dir_all(&dict_dir)
        .map_err(|e| format!("Failed to create dictionaries dir: {}", e))?;

    let file_path = dict_dir.join("characters.json");
    let json = serde_json::to_string_pretty(&characters)
        .map_err(|e| format!("Failed to serialize merged characters: {}", e))?;
    std::fs::write(&file_path, &json)
        .map_err(|e| format!("Failed to write {}: {}", file_path.display(), e))?;

    Ok(file_path.to_string_lossy().to_string())
}

/// Load merged characters from dictionaries/characters.json.
#[tauri::command]
pub fn load_merged_characters(dir: String) -> Result<Vec<MergedCharacter>, String> {
    let file_path = std::path::Path::new(&dir)
        .join("dictionaries")
        .join("characters.json");

    if !file_path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(&file_path)
        .map_err(|e| format!("Failed to read {}: {}", file_path.display(), e))?;
    serde_json::from_str(&content)
        .map_err(|e| format!("Failed to deserialize {}: {}", file_path.display(), e))
}

// ---------------------------------------------------------------------------
// merged_to_dictionary
// ---------------------------------------------------------------------------

/// Convert MergedCharacter list to the standard Character format used by the
/// translation pipeline. The output is saved as dictionaries/characters.json
/// so it can be loaded directly into AppState.characters.
#[tauri::command]
pub fn merged_to_dictionary(merged: Vec<MergedCharacter>) -> Vec<Character> {
    merged
        .iter()
        .map(|m| {
            let english_name = m
                .english_name
                .value
                .clone()
                .unwrap_or_else(|| "unknown".to_string());

            // Generate a slug-like id from the English name
            let id = english_name
                .to_lowercase()
                .replace(|c: char| !c.is_alphanumeric() && c != ' ', "")
                .split_whitespace()
                .collect::<Vec<_>>()
                .join("_");

            let japanese_name = if m.japanese_name.value.is_empty() {
                // Fall back to Chinese name if Japanese is not filled in
                m.chinese_name
                    .value
                    .clone()
                    .unwrap_or_else(|| english_name.clone())
            } else {
                m.japanese_name.value.clone()
            };

            let aliases = if m.aliases.is_empty() {
                // Include Chinese name as an alias if available
                m.chinese_name
                    .value
                    .clone()
                    .map(|cn| vec![cn])
                    .unwrap_or_default()
            } else {
                m.aliases.clone()
            };

            Character {
                id,
                english_name,
                chinese_name: m.chinese_name.value.clone(),
                japanese_name,
                aliases,
                role: m.role_type.value.clone(),
                status: None,
                gender: m.gender.clone(),
                default_register: "neutral".to_string(),
                speech_style: None,
                notes: m.review_note.clone(),
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// save_raw_html
// ---------------------------------------------------------------------------

/// Save raw HTML to the metadata/raw_pages/ directory.
#[tauri::command]
pub fn save_raw_html(dir: String, source: String, html: String) -> Result<String, String> {
    let raw_dir = std::path::Path::new(&dir)
        .join("metadata")
        .join("raw_pages");
    std::fs::create_dir_all(&raw_dir)
        .map_err(|e| format!("Failed to create raw_pages dir: {}", e))?;

    let safe_name = crate::scraper::sanitize_filename(&source);
    let file_path = raw_dir.join(format!("{}.html", safe_name));
    std::fs::write(&file_path, &html)
        .map_err(|e| format!("Failed to write {}: {}", file_path.display(), e))?;

    Ok(file_path.to_string_lossy().to_string())
}

// ---------------------------------------------------------------------------
// Character dict builder (paste-based, for sites that block automated scraping)
// ---------------------------------------------------------------------------

/// Parse text pasted from MyDramaList cast page.
#[tauri::command]
pub fn parse_mdl_paste(text: String) -> Vec<PastedEntry> {
    character_dict::parse_mdl_paste(&text)
}

/// Parse text pasted from Douban celebrities page.
#[tauri::command]
pub fn parse_douban_paste(text: String) -> Vec<PastedEntry> {
    character_dict::parse_douban_paste(&text)
}

/// Search TMDb for movies and TV shows matching the query.
#[tauri::command]
pub async fn search_tmdb(query: String) -> Result<Vec<crate::scraper::tmdb::TmdbSearchResult>, String> {
    scraper::tmdb::search_tmdb(&query).await
}

/// Fetch TMDb credits by TMDb ID and media type.
#[tauri::command]
pub async fn scrape_tmdb_credits(
    tmdb_id: u32,
    media_type: String,
) -> Result<ScrapeResult, String> {
    scraper::tmdb::fetch_tmdb_credits_by_id(tmdb_id, &media_type).await
}

/// Parse text pasted from IMDb or TMDb cast page.
#[tauri::command]
pub fn parse_tmdb_paste(text: String) -> Vec<PastedEntry> {
    character_dict::parse_tmdb_paste(&text)
}

/// Build a character dictionary from IMDb/TMDb + Douban pasted entries,
/// keyed by actor's English name.
#[tauri::command]
pub fn build_character_dict(
    imdb_entries: Vec<PastedEntry>,
    douban_entries: Vec<PastedEntry>,
) -> CharacterDict {
    character_dict::build_character_dict(&imdb_entries, &douban_entries)
}

/// Verify a character dictionary and return a quality report.
#[tauri::command]
pub fn verify_character_dict(dict: CharacterDict) -> QualityReport {
    character_dict::verify_character_dict(&dict)
}

// ---------------------------------------------------------------------------
// MDL WebView DOM extraction
// ---------------------------------------------------------------------------

use crate::scraper::mydramalist::{MdlExtractResult, MdlExtractState, MdlPageInfo, MdlPageInfoState};
use tauri::Manager;

/// Open a dedicated Tauri WebView window to display an MDL page.
/// If a window with label "mdl-browser" already exists, navigates it to the URL instead.
#[tauri::command]
pub fn open_mdl_window(app: tauri::AppHandle, url: String) -> Result<(), String> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    println!("[MDL] open_mdl_window called with url={}", url);

    if let Some(w) = app.get_webview_window("mdl-browser") {
        println!("[MDL] window 'mdl-browser' already exists, navigating");
        w.eval(&format!(
            "window.location.href = '{}';",
            url.replace('\'', "\\'")
        ))
        .map_err(|e| e.to_string())?;
        w.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    let parsed_url: url::Url = url.parse().map_err(|e| format!("Invalid URL: {}", e))?;
    println!("[MDL] creating new WebView window for {}", parsed_url);
    let win = WebviewWindowBuilder::new(&app, "mdl-browser", WebviewUrl::External(parsed_url))
        .title("MDL — ページを表示してください")
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36")
        .inner_size(1200.0, 900.0)
        .enable_clipboard_access()
        .devtools(true)
        .build()
        .map_err(|e| format!("[MDL] window build error: {}", e))?;
    println!("[MDL] window created successfully, opening devtools");
    win.open_devtools();

    // Force-close the window when the user clicks X, even if the external
    // page's JavaScript tries to prevent it via beforeunload.
    let win_handle = win.clone();
    win.on_window_event(move |event| {
        if let tauri::WindowEvent::CloseRequested { .. } = event {
            println!("[MDL] close requested via X button, closing window");
            let _ = win_handle.close();
        }
    });

    Ok(())
}

/// Receive the full HTML of the currently displayed MDL page (sent via eval/invoke),
/// parse it, and store the result in MdlExtractState.
#[tauri::command]
pub fn receive_mdl_extract(
    state: State<'_, MdlExtractState>,
    html: String,
) -> Result<(), String> {
    println!("[MDL] receive_mdl_extract called, html length={} bytes", html.len());
    let result = scraper::mydramalist::parse_mdl_html(&html);
    match &result {
        Ok(r) => println!(
            "[MDL] parse_mdl_html OK: title={:?}, synopsis={}, entries={}",
            r.title,
            r.synopsis.as_ref().map(|s| s.len()).unwrap_or(0),
            r.entries.len()
        ),
        Err(e) => println!("[MDL] parse_mdl_html FAILED: {}", e),
    }
    let result = result?;
    *state.0.lock().unwrap() = Some(result);
    Ok(())
}

/// Retrieve (and consume) the latest MDL extraction result.
/// Returns None if nothing has been extracted yet.
#[tauri::command]
pub fn get_mdl_extract(
    state: State<'_, MdlExtractState>,
) -> Result<Option<MdlExtractResult>, String> {
    let result = state.0.lock().unwrap().take();
    println!("[MDL] get_mdl_extract called, has_result={}", result.is_some());
    Ok(result)
}

/// Run a DOM-extraction script inside the MDL WebView window.
/// The script parses the page DOM and sends the HTML back to `receive_mdl_extract` via invoke.
#[tauri::command]
pub fn extract_mdl_data(app: tauri::AppHandle) -> Result<(), String> {
    println!("[MDL] extract_mdl_data called");
    let win = app
        .get_webview_window("mdl-browser")
        .ok_or("MDLウィンドウが見つかりません。先に「MDLを開く」を実行してください。")?;
    println!("[MDL] mdl-browser window found, executing eval");

    win.eval(
        r#"
        (async () => {
          try {
            if (typeof window.__TAURI__ === 'undefined') {
              document.title = '__MDL_NO_TAURI__';
              return;
            }
            const html = document.documentElement.outerHTML;
            await window.__TAURI__.core.invoke('receive_mdl_extract', { html: html });
          } catch (e) {
            document.title = '__MDL_EXTRACT_ERROR__:' + String(e);
          }
        })()
        "#,
    )
    .map_err(|e| {
        println!("[MDL] eval FAILED: {}", e);
        format!("eval実行エラー: {}", e)
    })?;
    println!("[MDL] eval sent successfully (async invoke pending)");
    Ok(())
}

/// Run a diagnostic inspection script inside the MDL WebView.
/// Results are sent back via `receive_mdl_page_info`.
#[tauri::command]
pub fn inspect_mdl_page(app: tauri::AppHandle) -> Result<(), String> {
    println!("[MDL] inspect_mdl_page called");
    let win = app
        .get_webview_window("mdl-browser")
        .ok_or("MDLウィンドウが見つかりません。先に「MDLを開く」を実行してください。")?;
    println!("[MDL] mdl-browser window found for inspection, executing eval");

    win.eval(
        r#"
        (async () => {
          try {
            if (typeof window.__TAURI__ === 'undefined') {
              document.title = '__MDL_NO_TAURI__';
              return;
            }
            const info = {
              url: location.href,
              title: document.title,
              body_preview: (document.body?.innerText || '').substring(0, 500),
              has_tauri: true,
              body_length: (document.body?.innerText || '').length,
            };
            await window.__TAURI__.core.invoke('receive_mdl_page_info', { info: info });
          } catch (e) {
            document.title = '__MDL_INSPECT_ERROR__:' + String(e);
          }
        })()
        "#,
    )
    .map_err(|e| {
        println!("[MDL] inspect eval FAILED: {}", e);
        format!("eval実行エラー: {}", e)
    })?;
    println!("[MDL] inspect eval sent successfully");
    Ok(())
}

/// Receive diagnostic info from the MDL webview.
#[tauri::command]
pub fn receive_mdl_page_info(
    state: State<'_, MdlPageInfoState>,
    info: MdlPageInfo,
) -> Result<(), String> {
    println!(
        "[MDL] receive_mdl_page_info: url={}, title='{}', body_len={}, has_tauri={}",
        info.url, info.title, info.body_length, info.has_tauri
    );
    *state.0.lock().unwrap() = Some(info);
    Ok(())
}

/// Retrieve (and consume) the latest MDL page inspection result.
#[tauri::command]
pub fn get_mdl_page_info(
    state: State<'_, MdlPageInfoState>,
) -> Result<Option<MdlPageInfo>, String> {
    let result = state.0.lock().unwrap().take();
    println!("[MDL] get_mdl_page_info called, has_result={}", result.is_some());
    Ok(result)
}

/// Close the MDL browser window.
#[tauri::command]
pub fn close_mdl_window(app: tauri::AppHandle) -> Result<(), String> {
    println!("[MDL] close_mdl_window called");
    let found = app.get_webview_window("mdl-browser");
    println!("[MDL] close: window_found={}", found.is_some());
    match found {
        Some(w) => {
            match w.close() {
                Ok(()) => {
                    println!("[MDL] close: result=ok");
                    Ok(())
                }
                Err(e) => {
                    println!("[MDL] close: result=fail error={}", e);
                    Err(format!("[MDL] window close error: {}", e))
                }
            }
        }
        None => {
            println!("[MDL] close: result=skip (no window)");
            Ok(())
        }
    }
}
