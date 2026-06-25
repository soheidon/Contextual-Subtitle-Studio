pub mod douban;
pub mod mydramalist;
pub mod tmdb;
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
                "en-US,en;q=0.9,zh-CN;q=0.8,ja;q=0.7".parse().unwrap(),
            );
            headers.insert(
                reqwest::header::ACCEPT_ENCODING,
                "gzip, deflate, br".parse().unwrap(),
            );
            headers.insert(reqwest::header::CACHE_CONTROL, "no-cache".parse().unwrap());
            headers.insert(reqwest::header::PRAGMA, "no-cache".parse().unwrap());
            headers.insert(
                "sec-ch-ua",
                "\"Google Chrome\";v=\"125\", \"Chromium\";v=\"125\", \"Not.A/Brand\";v=\"24\""
                    .parse()
                    .unwrap(),
            );
            headers.insert("sec-ch-ua-mobile", "?0".parse().unwrap());
            headers.insert("sec-ch-ua-platform", "\"Windows\"".parse().unwrap());
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

// ---------------------------------------------------------------------------
// Search candidate scoring
// ---------------------------------------------------------------------------

/// A candidate result from a database search (Douban, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchCandidate {
    pub url: String,
    pub title: String,
    pub year: Option<String>,
    pub confidence: f64,
    pub reason: String,
}

/// Score a search candidate against the query titles.
///
/// Returns (confidence, reason).
pub fn score_search_candidate(
    query_zh: &str,
    query_en: &str,
    aliases: &[String],
    candidate_title: &str,
    candidate_year: &Option<String>,
    expected_year: &Option<String>,
) -> (f64, String) {
    let q_zh = query_zh.trim();
    let q_en = query_en.trim().to_lowercase();
    let cand = candidate_title.trim();
    let cand_lower = cand.to_lowercase();

    // Exact Chinese title match
    if !q_zh.is_empty() && cand.contains(q_zh) {
        return (1.0, "title_exact_zh".to_string());
    }

    // Exact English title match
    if !q_en.is_empty() && (cand_lower == q_en || cand_lower.contains(&q_en)) {
        let year_match = match (candidate_year, expected_year) {
            (Some(cy), Some(ey)) => cy == ey,
            _ => false,
        };
        if year_match {
            return (0.95, "title_exact_en+year".to_string());
        }
        return (0.85, "title_exact_en".to_string());
    }

    // Alias match
    for alias in aliases {
        let a = alias.trim().to_lowercase();
        if !a.is_empty() && (cand_lower == a || cand_lower.contains(&a)) {
            return (0.80, "alias_match".to_string());
        }
    }

    // Partial match: check if any significant word overlaps
    let en_words: Vec<&str> = q_en.split_whitespace().collect();
    if en_words.len() >= 2 {
        let matching = en_words.iter().filter(|w| cand_lower.contains(**w)).count();
        if matching >= en_words.len() {
            return (0.70, "partial_match_all_words".to_string());
        }
        if matching >= en_words.len() / 2 {
            return (0.50, "partial_match_some_words".to_string());
        }
    }

    (0.0, "no_match".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_score_exact_zh() {
        let (conf, reason) = score_search_candidate(
            "冰湖重生",
            "Rebirth",
            &[],
            "冰湖重生 (2025)",
            &Some("2025".into()),
            &Some("2025".into()),
        );
        assert_eq!(conf, 1.0);
        assert_eq!(reason, "title_exact_zh");
    }

    #[test]
    fn test_score_exact_en_with_year() {
        let (conf, reason) = score_search_candidate(
            "",
            "Rebirth",
            &[],
            "Rebirth",
            &Some("2025".into()),
            &Some("2025".into()),
        );
        assert_eq!(conf, 0.95);
        assert_eq!(reason, "title_exact_en+year");
    }

    #[test]
    fn test_score_exact_en_no_year() {
        let (conf, reason) = score_search_candidate("", "Rebirth", &[], "Rebirth", &None, &None);
        assert_eq!(conf, 0.85);
        assert_eq!(reason, "title_exact_en");
    }

    #[test]
    fn test_score_alias_match() {
        let (conf, reason) = score_search_candidate(
            "",
            "X",
            &["Frozen Awakening".into()],
            "Frozen Awakening",
            &None,
            &None,
        );
        assert_eq!(conf, 0.80);
        assert_eq!(reason, "alias_match");
    }

    #[test]
    fn test_score_no_match() {
        let (conf, reason) = score_search_candidate(
            "冰湖重生",
            "Rebirth",
            &[],
            "Something Completely Different",
            &None,
            &None,
        );
        assert_eq!(conf, 0.0);
        assert_eq!(reason, "no_match");
    }
}
