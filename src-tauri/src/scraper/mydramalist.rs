use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;

use super::{ScrapedCharacter, ScrapeResult, ScrapeSource};
use crate::character_dict::{PasteSource, PastedEntry};

/// Result of extracting data from a WebView-loaded MDL page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MdlExtractResult {
    pub title: Option<String>,
    pub synopsis: Option<String>,
    pub entries: Vec<PastedEntry>,
}

/// Shared state for storing the latest MDL extraction result.
pub struct MdlExtractState(pub Mutex<Option<MdlExtractResult>>);

/// Diagnostic info about the currently displayed MDL page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MdlPageInfo {
    pub url: String,
    pub title: String,
    pub body_preview: String,
    pub has_tauri: bool,
    pub body_length: usize,
}

/// Shared state for the latest MDL page inspection result.
pub struct MdlPageInfoState(pub Mutex<Option<MdlPageInfo>>);

const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36";

/// Fetch MDL page with browser-like headers to try bypassing Cloudflare.
async fn fetch_mdl(url: &str) -> Result<String, String> {
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .default_headers({
            let mut h = reqwest::header::HeaderMap::new();
            h.insert(
                reqwest::header::ACCEPT,
                "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8"
                    .parse().unwrap(),
            );
            h.insert(
                reqwest::header::ACCEPT_LANGUAGE,
                "en-US,en;q=0.9".parse().unwrap(),
            );
            h.insert(
                reqwest::header::ACCEPT_ENCODING,
                "gzip, deflate, br".parse().unwrap(),
            );
            h.insert("sec-ch-ua", "\"Chromium\";v=\"125\", \"Google Chrome\";v=\"125\"".parse().unwrap());
            h.insert("sec-ch-ua-mobile", "?0".parse().unwrap());
            h.insert("sec-ch-ua-platform", "\"Windows\"".parse().unwrap());
            h.insert("sec-fetch-dest", "document".parse().unwrap());
            h.insert("sec-fetch-mode", "navigate".parse().unwrap());
            h.insert("sec-fetch-site", "none".parse().unwrap());
            h.insert("sec-fetch-user", "?1".parse().unwrap());
            h.insert("upgrade-insecure-requests", "1".parse().unwrap());
            h
        })
        .build()
        .map_err(|e| format!("Failed to build HTTP client: {}", e))?;

    let response = client
        .get(url)
        .header(reqwest::header::REFERER, "https://www.google.com/")
        .timeout(std::time::Duration::from_secs(30))
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    let status = response.status();
    let headers: Vec<(String, String)> = response
        .headers()
        .iter()
        .filter_map(|(k, v)| v.to_str().ok().map(|v| (k.to_string(), v.to_string())))
        .collect();

    if !status.is_success() {
        let body_preview = response.text().await.unwrap_or_default();
        let snippet: String = body_preview.chars().take(2000).collect();
        let header_str = headers
            .iter()
            .map(|(k, v)| format!("{}: {}", k, v))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(format!(
            "HTTP {} ({})\n\n--- Response Headers ---\n{}\n\n--- Body (先頭2000文字) ---\n{}",
            status.as_u16(),
            status.canonical_reason().unwrap_or("Unknown"),
            header_str,
            snippet
        ));
    }

    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    // Cloudflare challenge ページの検出
    if body.contains("cf-challenge") || body.contains("Just a moment") || body.contains("Checking if the site connection is secure") {
        let snippet: String = body.chars().take(2000).collect();
        return Err(format!(
            "Cloudflare challenge 検出 (HTTP 200 だがチャレンジページ)\n\n--- Body (先頭2000文字) ---\n{}",
            snippet
        ));
    }

    Ok(body)
}

/// Scrape MyDramaList cast page.
/// URL pattern: https://mydramalist.com/XXXXX-title/cast
///
/// MDL uses Cloudflare protection. We try with browser-like headers.
/// If blocked, the app should offer manual paste as fallback.
pub async fn scrape_mydramalist(url: &str) -> Result<ScrapeResult, String> {
    let html = fetch_mdl(url).await?;
    let document = Html::parse_document(&html);

    // Extract page title
    let title_sel = Selector::parse("title").unwrap();
    let page_title = document
        .select(&title_sel)
        .next()
        .map(|e| e.text().collect::<String>().trim().to_string());

    // Extract drama title from breadcrumb or heading
    let drama_title = extract_drama_title(&document).or_else(|| {
        page_title
            .as_ref()
            .and_then(|t| t.split(" cast - MyDramaList").next())
            .map(|s| s.trim().to_string())
    });

    // Extract synopsis
    let synopsis = extract_synopsis(&document);

    // Try primary selector for cast rows
    let mut characters = extract_cast_primary(&document);
    if characters.is_empty() {
        characters = extract_cast_fallback(&document);
    }

    if characters.is_empty() {
        return Err(format!(
            "MyDramaList: キャスト情報が見つかりませんでした。\nURL: {}\nページタイトル: {:?}",
            url, page_title
        ));
    }

    // Extract drama title from page
    Ok(ScrapeResult {
        source: ScrapeSource::MyDramaList,
        url: url.to_string(),
        page_title,
        drama_title,
        synopsis,
        characters,
        saved_html_path: None,
    })
}

fn extract_drama_title(document: &Html) -> Option<String> {
    // Try breadcrumb: <ol class="breadcrumb"> ... <li class="active">
    let breadcrumb = Selector::parse(".breadcrumb .active, .breadcrumb li:last-child").ok()?;
    document
        .select(&breadcrumb)
        .next()
        .map(|e| e.text().collect::<String>().trim().to_string())
}

fn extract_synopsis(document: &Html) -> Option<String> {
    let sel = Selector::parse(".show-synopsis p, .synopsis p, .show-synopsis").ok()?;
    document
        .select(&sel)
        .next()
        .map(|e| e.text().collect::<String>().trim().to_string())
}

/// Primary cast extraction: look for cast list items.
fn extract_cast_primary(document: &Html) -> Vec<ScrapedCharacter> {
    let row_sel = Selector::parse(".cast li, .cast-crew li, .list-item, tr.cast").unwrap();
    let name_sel = Selector::parse("h3 a, .text-primary a, .title a, a.text-primary").unwrap();
    let actor_sel =
        Selector::parse(".text-muted, small, .actor-name, .muted").unwrap();

    let mut chars = Vec::new();
    for (i, row) in document.select(&row_sel).enumerate() {
        let character_name = row
            .select(&name_sel)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string());

        let actor_name = row
            .select(&actor_sel)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string());

        if let Some(name) = character_name {
            chars.push(ScrapedCharacter {
                source_id: format!("cast_{:03}", i),
                character_name: name,
                actor_name,
                role_type: None,
                aliases: Vec::new(),
            });
        }
    }
    chars
}

/// Fallback: broader selectors when the primary ones don't match.
fn extract_cast_fallback(document: &Html) -> Vec<ScrapedCharacter> {
    // Try any <li> inside a container with "cast" or "credit" in class/id
    let row_sel = Selector::parse("[class*='cast'] li, [id*='cast'] li, [class*='credit'] li")
        .unwrap();
    let link_sel = Selector::parse("a").unwrap();

    let mut chars = Vec::new();
    for (i, row) in document.select(&row_sel).enumerate() {
        let links: Vec<String> = row
            .select(&link_sel)
            .map(|e| e.text().collect::<String>().trim().to_string())
            .collect();

        if links.len() >= 2 {
            chars.push(ScrapedCharacter {
                source_id: format!("cast_{:03}", i),
                character_name: links[1].clone(), // Usually character name is the second link
                actor_name: Some(links[0].clone()),
                role_type: None,
                aliases: Vec::new(),
            });
        } else if links.len() == 1 {
            chars.push(ScrapedCharacter {
                source_id: format!("cast_{:03}", i),
                character_name: links[0].clone(),
                actor_name: None,
                role_type: None,
                aliases: Vec::new(),
            });
        }
    }
    chars
}

/// Parse raw MDL HTML into an MdlExtractResult.
/// Reuses the same selector logic as `scrape_mydramalist` but without fetching.
pub fn parse_mdl_html(html: &str) -> Result<MdlExtractResult, String> {
    if html.contains("cf-challenge")
        || html.contains("Just a moment")
        || html.contains("Checking if the site connection is secure")
        || html.contains("challenges.cloudflare.com")
        || html.contains("cf-mitigated")
        || html.contains("challenge-platform")
        || html.contains("Verifying you are human")
        || html.contains("Waiting for mydramalist.com to respond")
    {
        return Err("まだMDL本文が表示されていません。ページ表示後に再度抽出してください。".into());
    }

    let document = Html::parse_document(html);

    let title_sel = Selector::parse("title").unwrap();
    let page_title: Option<String> = document
        .select(&title_sel)
        .next()
        .map(|e| e.text().collect::<String>().trim().to_string());

    let title = extract_drama_title(&document).or_else(|| {
        page_title
            .as_ref()
            .and_then(|t| t.split(" - MyDramaList").next())
            .map(|s| s.trim().to_string())
    });

    let synopsis = extract_synopsis(&document);

    let mut characters = extract_cast_primary(&document);
    if characters.is_empty() {
        characters = extract_cast_fallback(&document);
    }

    let entries: Vec<PastedEntry> = characters
        .into_iter()
        .map(|c| PastedEntry {
            actor_name: c.actor_name.unwrap_or_default(),
            character_name: c.character_name,
            role_type: c.role_type,
            source: PasteSource::MyDramaList,
        })
        .collect();

    Ok(MdlExtractResult {
        title,
        synopsis,
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_from_empty_html() {
        let document = Html::parse_document("<html><body></body></html>");
        let chars = extract_cast_primary(&document);
        assert!(chars.is_empty());
    }

    #[test]
    fn test_extract_fallback_from_empty() {
        let document = Html::parse_document("<html><body></body></html>");
        let chars = extract_cast_fallback(&document);
        assert!(chars.is_empty());
    }

    /// Diagnostic: fetch actual MDL page and print raw HTML structure.
    /// Run with: cargo test scraper::mydramalist::tests::diagnose_mdl -- --nocapture --ignored
    #[tokio::test]
    #[ignore]
    async fn diagnose_mdl() {
        let url = "https://mydramalist.com/766289-frozen-awakening/cast";
        let html = fetch_mdl(url).await.expect("fetch failed");
        // Print first 8000 chars to see structure
        println!("=== RAW HTML (first 8000 chars) ===");
        println!("{}", &html[..html.len().min(8000)]);

        let document = Html::parse_document(&html);
        let page_title = document
            .select(&Selector::parse("title").unwrap())
            .next()
            .map(|e| e.text().collect::<String>());
        println!("\n=== PAGE TITLE: {:?} ===", page_title);

        // Dump all elements with 'cast' in class/id
        let cast_sel = Selector::parse("[class*='cast'], [id*='cast']").unwrap();
        println!("\n=== ELEMENTS WITH 'cast' in class/id ===");
        for el in document.select(&cast_sel) {
            let tag = el.value().name();
            let id = el.value().id().unwrap_or("");
            let classes: Vec<_> = el.value().classes().collect();
            let text = el.text().collect::<String>();
            let short = if text.len() > 200 { &text[..200] } else { &text };
            println!("  <{} id={:?} classes={:?}> text={:?}", tag, id, classes, short);
        }

        // Try current selectors
        let chars = extract_cast_primary(&document);
        println!("\n=== PRIMARY EXTRACTION: {} chars ===", chars.len());
        for c in &chars {
            println!("  {:?}", c);
        }

        let chars2 = extract_cast_fallback(&document);
        println!("\n=== FALLBACK EXTRACTION: {} chars ===", chars2.len());
        for c in &chars2 {
            println!("  {:?}", c);
        }
    }
}
