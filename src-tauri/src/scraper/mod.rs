pub mod douban;
pub mod tmdb;
pub mod mydramalist;
pub mod tvmao;

use serde::{Deserialize, Serialize};

/// A character extracted from a single source page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapedCharacter {
    /// Internal key within the source, e.g. "cast_001"
    pub source_id: String,
    /// Character name as it appears on the source page
    /// (English for MyDramaList, Chinese for 电视猫 / 豆瓣)
    pub character_name: String,
    pub actor_name: Option<String>,
    /// "Main Role" / "Support Role" / "Guest Role" etc.
    pub role_type: Option<String>,
    /// Alternate names found on the page
    pub aliases: Vec<String>,
}

/// The result of scraping one URL.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrapeResult {
    pub source: ScrapeSource,
    pub url: String,
    /// The HTML <title> of the page
    pub page_title: Option<String>,
    /// Clean drama title (extracted from page structure)
    pub drama_title: Option<String>,
    /// Brief synopsis if available
    pub synopsis: Option<String>,
    pub characters: Vec<ScrapedCharacter>,
    /// Path to raw HTML saved on disk
    pub saved_html_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ScrapeSource {
    MyDramaList,
    TvMao,
    Douban,
    Tmdb,
    Other(String),
}

/// Shared User-Agent for all HTTP requests.
pub const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36";

/// Fetch a URL and return the HTML body as a string.
pub async fn fetch_html(url: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .default_headers({
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert(
                reqwest::header::ACCEPT,
                "text/html,application/xhtml+xml,application/xml;q=0.9,image/webp,*/*;q=0.8"
                    .parse()
                    .unwrap(),
            );
            headers.insert(
                reqwest::header::ACCEPT_LANGUAGE,
                "en-US,en;q=0.9,zh-CN;q=0.8,ja;q=0.7"
                    .parse()
                    .unwrap(),
            );
            headers.insert(
                reqwest::header::ACCEPT_ENCODING,
                "gzip, deflate, br".parse().unwrap(),
            );
            headers.insert(
                reqwest::header::CACHE_CONTROL,
                "no-cache".parse().unwrap(),
            );
            headers.insert(
                reqwest::header::PRAGMA,
                "no-cache".parse().unwrap(),
            );
            headers.insert(
                "sec-ch-ua",
                "\"Google Chrome\";v=\"125\", \"Chromium\";v=\"125\", \"Not.A/Brand\";v=\"24\""
                    .parse()
                    .unwrap(),
            );
            headers.insert(
                "sec-ch-ua-mobile",
                "?0".parse().unwrap(),
            );
            headers.insert(
                "sec-ch-ua-platform",
                "\"Windows\"".parse().unwrap(),
            );
            headers
        })
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let response = client
        .get(url)
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    let status = response.status();
    if !status.is_success() {
        return Err(format!(
            "HTTP {}: server returned error status for {}",
            status, url
        ));
    }

    response
        .text()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))
}

/// Sanitize a string for use as a filename component.
pub fn sanitize_filename(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_whitespace() => '_',
            c => c,
        })
        .collect()
}
