use serde::{Deserialize, Serialize};

use super::{ScrapedCharacter, ScrapeResult, ScrapeSource};

const API_BASE: &str = "https://api.themoviedb.org/3";
const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36";

fn api_key() -> Result<String, String> {
    std::env::var("TMDB_API_KEY")
        .map_err(|_| "TMDB_API_KEY が未設定です。設定ページで TMDb API キーを登録してください。"
            .to_string())
}

// ---------------------------------------------------------------------------
// Serialized types
// ---------------------------------------------------------------------------

/// A single search result from TMDb search/multi.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TmdbSearchResult {
    pub tmdb_id: u32,
    pub title: String,
    pub original_title: Option<String>,
    pub media_type: String,
    pub year: Option<String>,
    pub overview: Option<String>,
}

// ---------------------------------------------------------------------------
// TMDb API JSON types (internal deserialization-only)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct TmdbSearchResponse {
    results: Vec<TmdbSearchItem>,
}

#[derive(Debug, Deserialize)]
struct TmdbSearchItem {
    id: u32,
    title: Option<String>,
    name: Option<String>,
    #[allow(dead_code)]
    original_title: Option<String>,
    #[allow(dead_code)]
    original_name: Option<String>,
    #[allow(dead_code)]
    overview: Option<String>,
    #[allow(dead_code)]
    media_type: String,
    #[allow(dead_code)]
    release_date: Option<String>,
    #[allow(dead_code)]
    first_air_date: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TmdbFindResponse {
    movie_results: Vec<TmdbFindResult>,
    tv_results: Vec<TmdbFindResult>,
}

#[derive(Debug, Deserialize)]
struct TmdbFindResult {
    id: u32,
    #[allow(dead_code)]
    title: Option<String>,
    #[allow(dead_code)]
    name: Option<String>,
    #[allow(dead_code)]
    overview: Option<String>,
    #[allow(dead_code)]
    media_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TmdbCreditsResponse {
    cast: Vec<TmdbCastMember>,
}

#[derive(Debug, Deserialize)]
struct TmdbCastMember {
    name: String,
    character: String,
    #[allow(dead_code)]
    order: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct TmdbDetailsResponse {
    title: Option<String>,
    name: Option<String>,
    overview: Option<String>,
}

// ---------------------------------------------------------------------------
// TMDb API fetch helpers
// ---------------------------------------------------------------------------

pub fn validate_api_key() -> Result<(), String> {
    api_key()?;
    Ok(())
}

fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .expect("failed to build reqwest client")
}

async fn tmdb_get(path: &str) -> Result<String, String> {
    let key = api_key()?;
    let url = format!("{}{}?api_key={}", API_BASE, path, key);
    let client = build_client();
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("TMDb API リクエスト失敗: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("TMDb API HTTP {} for {}", resp.status(), path));
    }

    resp.text()
        .await
        .map_err(|e| format!("TMDb API レスポンス読み取り失敗: {}", e))
}

// ---------------------------------------------------------------------------
// Search API
// ---------------------------------------------------------------------------

/// Search TMDb for movies and TV shows matching the query.
/// Returns a list of search results (both movie and tv, person excluded).
pub async fn search_tmdb(query: &str) -> Result<Vec<TmdbSearchResult>, String> {
    let key = api_key()?;
    let url = url::Url::parse_with_params(
        &format!("{}/search/multi", API_BASE),
        &[("query", query), ("language", "ja-JP"), ("api_key", &key)],
    )
    .map_err(|e| format!("URL生成失敗: {}", e))?;

    let client = build_client();
    let resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("TMDb search API 失敗: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("TMDb search HTTP {}", resp.status()));
    }

    let body = resp
        .text()
        .await
        .map_err(|e| format!("TMDb search レスポンス読み取り失敗: {}", e))?;

    let response: TmdbSearchResponse = serde_json::from_str(&body)
        .map_err(|e| format!("TMDb search パース失敗: {}", e))?;

    let results: Vec<TmdbSearchResult> = response
        .results
        .into_iter()
        .filter(|r| r.media_type == "movie" || r.media_type == "tv")
        .map(|r| {
            let title = r.title.or(r.name).unwrap_or_default();
            let original_title = r.original_title.or(r.original_name);
            let year = r
                .release_date
                .or(r.first_air_date)
                .map(|d| d.chars().take(4).collect());
            TmdbSearchResult {
                tmdb_id: r.id,
                title,
                original_title,
                media_type: r.media_type,
                year,
                overview: r.overview.filter(|o| !o.is_empty()),
            }
        })
        .collect();

    eprintln!(
        "[TMDb] search_tmdb: query={}, results={}",
        query,
        results.len()
    );
    Ok(results)
}

// ---------------------------------------------------------------------------
// URL parsing (for IMDb/TMDb URL fallback input)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
enum TmdbMediaType {
    Movie,
    Tv,
}

impl TmdbMediaType {
    fn as_str(&self) -> &str {
        match self {
            TmdbMediaType::Movie => "movie",
            TmdbMediaType::Tv => "tv",
        }
    }
}

/// Parse an IMDb or TMDb URL.
fn parse_tmdb_url(
    url: &str,
) -> Result<(Option<String>, Option<TmdbMediaType>, Option<u32>), String> {
    let lower = url.to_lowercase();

    if lower.contains("imdb.com") {
        let re =
            regex::Regex::new(r"imdb\.com/title/(tt\d+)").map_err(|e| format!("regex error: {}", e))?;
        if let Some(caps) = re.captures(url) {
            return Ok((Some(caps[1].to_string()), None, None));
        }
        return Err(format!("IMDb URL から ID を抽出できませんでした: {}", url));
    }

    if lower.contains("themoviedb.org") {
        let re = regex::Regex::new(r"themoviedb\.org(?:/[a-z]{2})?/(movie|tv)/(\d+)")
            .map_err(|e| format!("regex error: {}", e))?;
        if let Some(caps) = re.captures(url) {
            let media_type = match &caps[1] {
                "tv" => TmdbMediaType::Tv,
                _ => TmdbMediaType::Movie,
            };
            let id: u32 = caps[2]
                .parse()
                .map_err(|_| format!("TMDb ID のパースに失敗: {}", url))?;
            return Ok((None, Some(media_type), Some(id)));
        }
        return Err(format!("TMDb URL から ID を抽出できませんでした: {}", url));
    }

    Err(format!(
        "未対応のURLです。IMDb (imdb.com) または TMDb (themoviedb.org) のURLを入力してください: {}",
        url
    ))
}

// ---------------------------------------------------------------------------
// Credits fetch (shared: used by both URL-based and ID-based flows)
// ---------------------------------------------------------------------------

async fn fetch_credits_for_tmdb_id(
    media_type: &str,
    tmdb_id: u32,
) -> Result<ScrapeResult, String> {
    // Fetch details for title/synopsis
    let details_json = tmdb_get(&format!("/{}/{}", media_type, tmdb_id)).await?;
    let details: TmdbDetailsResponse = serde_json::from_str(&details_json)
        .map_err(|e| format!("TMDb details パース失敗: {}", e))?;

    let drama_title = details.title.or(details.name);
    let synopsis = details.overview;
    eprintln!("[TMDb] Title: {:?}", drama_title);

    // Fetch credits
    let credits_json = tmdb_get(&format!("/{}/{}/credits", media_type, tmdb_id)).await?;
    let credits: TmdbCreditsResponse = serde_json::from_str(&credits_json)
        .map_err(|e| format!("TMDb credits パース失敗: {}", e))?;

    eprintln!("[TMDb] credits cast count: {}", credits.cast.len());

    let characters: Vec<ScrapedCharacter> = credits
        .cast
        .into_iter()
        .enumerate()
        .map(|(i, c)| ScrapedCharacter {
            source_id: format!("tmdb_cast_{:03}", i),
            character_name: if c.character.is_empty() {
                String::new()
            } else {
                c.character
            },
            actor_name: if c.name.is_empty() { None } else { Some(c.name) },
            role_type: Some("main".to_string()),
            aliases: Vec::new(),
        })
        .collect();

    let page_title = drama_title.clone();

    Ok(ScrapeResult {
        source: ScrapeSource::Tmdb,
        url: format!("https://www.themoviedb.org/{}/{}", media_type, tmdb_id),
        page_title,
        drama_title,
        synopsis,
        characters,
        saved_html_path: None,
    })
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Fetch credits by TMDb ID and media type (main route: search → select → credits).
pub async fn fetch_tmdb_credits_by_id(tmdb_id: u32, media_type: &str) -> Result<ScrapeResult, String> {
    let _ = api_key()?;
    eprintln!(
        "[TMDb] fetch_tmdb_credits_by_id: {}/{}",
        media_type, tmdb_id
    );
    fetch_credits_for_tmdb_id(media_type, tmdb_id).await
}

/// Fetch credits from an IMDb or TMDb URL (fallback route).
pub async fn scrape_tmdb_from_url(url: &str) -> Result<ScrapeResult, String> {
    let (imdb_id, media_type, tmdb_id) = parse_tmdb_url(url)?;
    let _ = api_key()?;

    if let Some(ref iid) = imdb_id {
        eprintln!("[TMDb] URL fallback: IMDb ID: {}", iid);
    }

    let (media, tmid) = if let Some(id) = tmdb_id {
        let media = media_type.unwrap_or(TmdbMediaType::Movie);
        (media.as_str().to_string(), id)
    } else if let Some(ref iid) = imdb_id {
        let find_json = tmdb_get(&format!("/find/{}?external_source=imdb_id", iid)).await?;
        let find: TmdbFindResponse = serde_json::from_str(&find_json)
            .map_err(|e| format!("TMDb find パース失敗: {}", e))?;

        let movie_count = find.movie_results.len();
        let tv_count = find.tv_results.len();
        eprintln!("[TMDb] TMDb find: tv_results={}, movie_results={}", tv_count, movie_count);

        let movie = find.movie_results.into_iter().next();
        let tv = find.tv_results.into_iter().next();
        match (movie, tv) {
            (Some(m), _) => {
                eprintln!("[TMDb] Matched as movie, TMDb ID: {}", m.id);
                ("movie".to_string(), m.id)
            }
            (_, Some(t)) => {
                eprintln!("[TMDb] Matched as tv, TMDb ID: {}", t.id);
                ("tv".to_string(), t.id)
            }
            (None, None) => {
                return Err(format!(
                    "IMDb ID '{}' に対応する TMDb 作品が見つかりません。",
                    iid
                ))
            }
        }
    } else {
        return Err("URL から IMDb ID または TMDb ID を抽出できませんでした".to_string());
    };

    fetch_credits_for_tmdb_id(&media, tmid).await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_imdb_url() {
        let result = parse_tmdb_url("https://www.imdb.com/title/tt1234567/");
        assert!(result.is_ok());
        let (imdb_id, media_type, tmdb_id) = result.unwrap();
        assert_eq!(imdb_id, Some("tt1234567".to_string()));
        assert!(media_type.is_none());
        assert!(tmdb_id.is_none());
    }

    #[test]
    fn test_parse_imdb_url_full() {
        let result = parse_tmdb_url("https://www.imdb.com/title/tt36809858/fullcredits");
        let (imdb_id, media_type, tmdb_id) = result.unwrap();
        assert_eq!(imdb_id, Some("tt36809858".to_string()));
        assert!(media_type.is_none());
        assert!(tmdb_id.is_none());
    }

    #[test]
    fn test_parse_tmdb_movie_url() {
        let result = parse_tmdb_url("https://www.themoviedb.org/movie/12345");
        let (imdb_id, media_type, tmdb_id) = result.unwrap();
        assert!(imdb_id.is_none());
        assert_eq!(media_type, Some(TmdbMediaType::Movie));
        assert_eq!(tmdb_id, Some(12345));
    }

    #[test]
    fn test_parse_tmdb_tv_url() {
        let result = parse_tmdb_url("https://www.themoviedb.org/tv/67890");
        let (imdb_id, media_type, tmdb_id) = result.unwrap();
        assert!(imdb_id.is_none());
        assert_eq!(media_type, Some(TmdbMediaType::Tv));
        assert_eq!(tmdb_id, Some(67890));
    }

    #[test]
    fn test_parse_tmdb_ja_url() {
        let result = parse_tmdb_url("https://www.themoviedb.org/ja/movie/12345-drama-name");
        let (imdb_id, media_type, tmdb_id) = result.unwrap();
        assert!(imdb_id.is_none());
        assert_eq!(media_type, Some(TmdbMediaType::Movie));
        assert_eq!(tmdb_id, Some(12345));
    }

    #[test]
    fn test_parse_invalid_url() {
        let result = parse_tmdb_url("https://example.com/movie/123");
        assert!(result.is_err());
    }
}
