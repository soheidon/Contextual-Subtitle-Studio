use regex::Regex;
use scraper::{Html, Selector};
use sha2::{Digest, Sha512};

use super::{
    score_search_candidate, ScrapeResult, ScrapeSource, ScrapedCharacter, SearchCandidate,
};

const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36";

/// Normalize any Douban subject URL to the /celebrities page.
///
/// Accepts:
/// - https://movie.douban.com/subject/36809858/
/// - https://movie.douban.com/subject/36809858
/// - https://movie.douban.com/subject/36809858/celebrities
///
/// Returns: https://movie.douban.com/subject/{id}/celebrities
pub fn normalize_douban_celebrities_url(url: &str) -> Result<String, String> {
    let re = Regex::new(r"douban\.com/subject/(\d+)").map_err(|e| format!("regex error: {}", e))?;
    let caps = re
        .captures(url)
        .ok_or_else(|| format!(
            "Douban subject IDを取得できません。movie.douban.com/subject/... URLを入力してください: {}",
            url
        ))?;
    let subject_id = &caps[1];
    let normalized = format!(
        "https://movie.douban.com/subject/{}/celebrities",
        subject_id
    );
    Ok(normalized)
}

/// Scrape 豆瓣 movie/drama page via celebrities page.
/// URL pattern: https://movie.douban.com/subject/XXXXX/celebrities
///
/// Douban uses a SHA-512 proof-of-work challenge for automated requests.
/// This scraper solves the challenge in Rust to obtain a session cookie,
/// then fetches the actual page content.
pub async fn scrape_douban(url: &str) -> Result<ScrapeResult, String> {
    let url = normalize_douban_celebrities_url(url)?;
    eprintln!("[Douban] celebrities URL: {}", url);

    eprintln!("[Douban] ページ取得開始");
    let html = fetch_douban_with_challenge_solve(&url).await?;
    let document = Html::parse_document(&html);

    if is_login_page(&document) {
        return Err(format!(
            "豆瓣: ログインが必要です。\nURL: {}\n\nブラウザでログイン後、再度お試しください。",
            url
        ));
    }

    let title_sel = Selector::parse("title").unwrap();
    let page_title = document
        .select(&title_sel)
        .next()
        .map(|e| e.text().collect::<String>().trim().to_string());

    let drama_title = extract_drama_title(&document);
    let synopsis = extract_synopsis(&document);

    let mut characters = extract_cast_primary(&document);
    eprintln!("[Douban] cast_primary検出: {}件", characters.len());

    if characters.is_empty() {
        characters = extract_cast_celebrities(&document);
        eprintln!("[Douban] cast_celebrities検出: {}件", characters.len());
    }
    if characters.is_empty() {
        characters = extract_cast_generic(&document);
        eprintln!("[Douban] cast_generic検出: {}件", characters.len());
    }

    eprintln!("[Douban] actor/character抽出成功: {}件", characters.len());

    if characters.is_empty() {
        return Err(format!(
            "豆瓣: キャスト情報が見つかりませんでした。\nURL: {}\nページタイトル: {:?}\n\n/celebrities ページを直接開いて、手動貼り付けを試してください。",
            url, page_title
        ));
    }

    Ok(ScrapeResult {
        source: ScrapeSource::Douban,
        url,
        page_title,
        drama_title,
        synopsis,
        characters,
        saved_html_path: None,
    })
}

/// Fetch a Douban page, solving the SHA-512 PoW challenge automatically.
async fn fetch_douban_with_challenge_solve(url: &str) -> Result<String, String> {
    let client = build_client();

    // Step 1: Initial request — may get challenge page
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("HTTP request failed: {}", e))?;

    let status = response.status();
    let body = response
        .text()
        .await
        .map_err(|e| format!("Failed to read body: {}", e))?;

    if status.is_success() {
        // Check if this is a challenge page
        if let Some((tok, cha, red)) = parse_challenge(&body) {
            // Solve the challenge
            let nonce = solve_challenge(&cha);
            let cookie = submit_challenge(&tok, &cha, nonce, &red).await?;

            // Step 3: Fetch actual page with the session cookies
            let client2 = build_client();
            let resp = client2
                .get(&red)
                .header("Cookie", &cookie)
                .send()
                .await
                .map_err(|e| format!("Failed to fetch after challenge: {}", e))?;

            if !resp.status().is_success() {
                return Err(format!(
                    "HTTP {} after challenge solve for {}",
                    resp.status(),
                    url
                ));
            }

            resp.text()
                .await
                .map_err(|e| format!("Failed to read body after challenge: {}", e))
        } else {
            // No challenge, return body directly
            Ok(body)
        }
    } else {
        Err(format!(
            "HTTP {}: server returned error status for {}",
            status, url
        ))
    }
}

fn build_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .default_headers({
            let mut h = reqwest::header::HeaderMap::new();
            h.insert(
                reqwest::header::ACCEPT,
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"
                    .parse()
                    .unwrap(),
            );
            h.insert(
                reqwest::header::ACCEPT_LANGUAGE,
                "zh-CN,zh;q=0.9,en;q=0.8".parse().unwrap(),
            );
            h
        })
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .expect("failed to build reqwest client")
}

/// Parse the Douban challenge page to extract tok, cha, red parameters.
fn parse_challenge(body: &str) -> Option<(String, String, String)> {
    let doc = Html::parse_document(body);

    let tok = doc
        .select(&Selector::parse("#tok").ok()?)
        .next()?
        .value()
        .attr("value")?
        .to_string();

    let cha = doc
        .select(&Selector::parse("#cha").ok()?)
        .next()?
        .value()
        .attr("value")?
        .to_string();

    let red = doc
        .select(&Selector::parse("#red").ok()?)
        .next()?
        .value()
        .attr("value")?
        .to_string();

    Some((tok, cha, red))
}

/// Solve the SHA-512 proof-of-work challenge.
/// Difficulty is always 4 (find hash starting with "0000").
fn solve_challenge(cha: &str) -> u64 {
    let mut nonce: u64 = 0;
    loop {
        nonce += 1;
        let input = format!("{}{}", cha, nonce);
        let hash = hex::encode(Sha512::digest(input.as_bytes()));
        if hash.starts_with("0000") {
            return nonce;
        }
    }
}

/// Submit the solved challenge and return all cookies as a Cookie header value.
async fn submit_challenge(tok: &str, cha: &str, nonce: u64, red: &str) -> Result<String, String> {
    let form = [
        ("tok", tok),
        ("cha", cha),
        ("sol", &nonce.to_string()),
        ("red", red),
    ];

    // Don't follow redirects — we need to capture the Set-Cookie headers
    let client = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| format!("Failed to build client: {}", e))?;

    let resp = client
        .post("https://sec.douban.com/c")
        .form(&form)
        .send()
        .await
        .map_err(|e| format!("Challenge submit failed: {}", e))?;

    if resp.status().as_u16() != 302 {
        return Err(format!(
            "Expected 302 after challenge, got {}",
            resp.status()
        ));
    }

    // Collect all cookie name=value pairs
    let mut cookie_pairs: Vec<String> = Vec::new();
    for header_val in resp.headers().get_all(reqwest::header::SET_COOKIE) {
        if let Ok(s) = header_val.to_str() {
            // Take the first part before ';' (name=value)
            if let Some(pair) = s.split(';').next() {
                cookie_pairs.push(pair.trim().to_string());
            }
        }
    }

    if cookie_pairs.is_empty() {
        return Err("No cookies received after challenge".to_string());
    }

    Ok(cookie_pairs.join("; "))
}

fn is_login_page(document: &Html) -> bool {
    let html_text = document.root_element().text().collect::<String>();
    html_text.contains("登录豆瓣") || html_text.contains("请登录") || html_text.contains("需要登录")
}

fn extract_drama_title(document: &Html) -> Option<String> {
    let sel = Selector::parse("h1 span[property='v:itemreviewed'], h1").ok()?;
    document
        .select(&sel)
        .next()
        .map(|e| e.text().collect::<String>().trim().to_string())
}

fn extract_synopsis(document: &Html) -> Option<String> {
    let sel = Selector::parse(
        "#link-report-intra span[property='v:summary'], #link-report span, .related-info .indent span",
    )
    .ok()?;
    document
        .select(&sel)
        .next()
        .map(|e| e.text().collect::<String>().trim().to_string())
}

fn extract_cast_primary(document: &Html) -> Vec<ScrapedCharacter> {
    let row_sel = Selector::parse(
        "#celebrities .celebrity, .casting_list li, ul.celebrity-list li, ul.celebrities-list li",
    )
    .unwrap();
    let name_sel = Selector::parse(".name a, a[rel='v:starring']").unwrap();
    let role_sel = Selector::parse(".role, em, .character").unwrap();

    let mut chars = Vec::new();
    for (i, row) in document.select(&row_sel).enumerate() {
        let raw_actor = row
            .select(&name_sel)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string());

        let raw_role = row
            .select(&role_sel)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string());

        // Parse actor name: "黄杨钿甜 Tiantian Huangyang" → (cn="黄杨钿甜", en="Tiantian Huangyang")
        let (actor_cn, actor_en) = raw_actor
            .as_deref()
            .map(split_cn_en_name)
            .unwrap_or((None, None));

        // Parse role: "演员 Actress (饰 楚乔)" → "楚乔", skip non-actors
        let role_type = raw_role.as_deref().map(classify_douban_role);
        let is_actor = matches!(role_type, Some(DoubanRoleType::Actor));
        let character_name = raw_role.as_deref().and_then(extract_douban_character_name);

        // Skip directors, writers, producers, etc.
        if is_actor && (actor_cn.is_some() || actor_en.is_some()) {
            let cn_for_alias = actor_cn.clone();
            chars.push(ScrapedCharacter {
                source_id: format!("cast_{:03}", i),
                character_name: character_name.unwrap_or_default(),
                actor_name: actor_en.or(actor_cn),
                role_type: role_type.map(|r| format!("{:?}", r)),
                aliases: cn_for_alias.map(|cn| vec![cn]).unwrap_or_default(),
            });
        }
    }
    chars
}

#[derive(Debug)]
enum DoubanRoleType {
    Actor,
    Director,
    Writer,
    Producer,
    Other,
}

fn classify_douban_role(role_text: &str) -> DoubanRoleType {
    if role_text.contains("导演") {
        DoubanRoleType::Director
    } else if role_text.contains("编剧") {
        DoubanRoleType::Writer
    } else if role_text.contains("制片") {
        DoubanRoleType::Producer
    } else if role_text.contains("演员") {
        DoubanRoleType::Actor
    } else {
        DoubanRoleType::Other
    }
}

/// Extract character name from Douban role text like "演员 Actress (饰 楚乔)" → "楚乔"
fn extract_douban_character_name(role_text: &str) -> Option<String> {
    // Pattern: "演员 Actor (饰 楚乔)" or "演员 Actress (饰 诸葛玥)"
    if let Some(start) = role_text.find("饰 ") {
        let after = &role_text[start + "饰 ".len()..];
        let end = after.find(')').unwrap_or(after.len());
        let name = after[..end].trim();
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }
    if let Some(start) = role_text.find("饰：") {
        let after = &role_text[start + "饰：".len()..];
        let end = after.find(')').unwrap_or(after.len());
        let name = after[..end].trim();
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }
    None
}

/// Split "黄杨钿甜 Tiantian Huangyang" into (Some("黄杨钿甜"), Some("Tiantian Huangyang"))
fn split_cn_en_name(raw: &str) -> (Option<String>, Option<String>) {
    // Find the boundary: Chinese characters end, ASCII starts
    let chars: Vec<char> = raw.chars().collect();
    let mut cn_end = 0;
    for (i, c) in chars.iter().enumerate() {
        if c.is_ascii() && !c.is_whitespace() {
            cn_end = i;
            break;
        }
    }
    if cn_end == 0 {
        // No ASCII found, whole string is Chinese
        return (Some(raw.trim().to_string()), None);
    }
    let cn = chars[..cn_end]
        .iter()
        .collect::<String>()
        .trim()
        .to_string();
    let en = chars[cn_end..]
        .iter()
        .collect::<String>()
        .trim()
        .to_string();
    if cn.is_empty() && en.is_empty() {
        (None, None)
    } else if cn.is_empty() {
        (None, Some(en))
    } else if en.is_empty() {
        (Some(cn), None)
    } else {
        (Some(cn), Some(en))
    }
}

fn extract_cast_celebrities(document: &Html) -> Vec<ScrapedCharacter> {
    let row_sel = Selector::parse(".celebrities-list .celebrity-item, div.celebrity").unwrap();
    let link_sel = Selector::parse("a").unwrap();

    let mut chars = Vec::new();
    for (i, row) in document.select(&row_sel).enumerate() {
        let links: Vec<String> = row
            .select(&link_sel)
            .map(|e| e.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if links.len() >= 2 {
            chars.push(ScrapedCharacter {
                source_id: format!("cast_{:03}", i),
                character_name: links[1].clone(),
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

fn extract_cast_generic(document: &Html) -> Vec<ScrapedCharacter> {
    let section_sel =
        Selector::parse("[class*='cast'], [id*='cast'], [class*='celebrity'], [id*='celebrity']")
            .unwrap();
    let link_sel = Selector::parse("a").unwrap();

    let mut chars = Vec::new();
    for section in document.select(&section_sel) {
        for (i, link) in section.select(&link_sel).enumerate() {
            let text = link.text().collect::<String>().trim().to_string();
            if !text.is_empty() && text.len() > 1 {
                chars.push(ScrapedCharacter {
                    source_id: format!("cast_{:03}", i),
                    character_name: text,
                    actor_name: None,
                    role_type: None,
                    aliases: Vec::new(),
                });
            }
        }
        if !chars.is_empty() {
            break;
        }
    }
    chars
}

// ---------------------------------------------------------------------------
// Douban search URL resolution
// ---------------------------------------------------------------------------

/// A raw search result parsed from the Douban search page.
struct DoubanSearchItem {
    url: String,
    title: String,
    year: Option<String>,
}

/// Search Douban for a drama/movie by title and return the best matching URL.
///
/// Returns (best_candidate, all_candidates) where best_candidate is the highest-scored item.
pub async fn search_douban_url(
    query_zh: &str,
    query_en: &str,
    aliases: &[String],
    expected_year: &Option<String>,
) -> Result<(Option<SearchCandidate>, Vec<SearchCandidate>), String> {
    let q_zh = query_zh.trim();
    let q_en = query_en.trim();

    // Try Chinese query first, then English if results are sparse
    let mut items = if !q_zh.is_empty() {
        eprintln!("[Douban] search: query_zh={}", q_zh);
        fetch_douban_search(q_zh).await?
    } else {
        Vec::new()
    };

    if items.len() < 3 && !q_en.is_empty() {
        eprintln!("[Douban] search: query_en={}", q_en);
        let en_items = fetch_douban_search(q_en).await?;
        // Merge, deduplicating by URL
        let existing_urls: std::collections::HashSet<String> =
            items.iter().map(|i| i.url.clone()).collect();
        for item in en_items {
            if !existing_urls.contains(&item.url) {
                items.push(item);
            }
        }
    }

    eprintln!("[Douban] search candidates: {}件", items.len());

    if items.is_empty() {
        return Ok((None, Vec::new()));
    }

    // Score each candidate
    let mut candidates: Vec<SearchCandidate> = items
        .iter()
        .map(|item| {
            let (confidence, reason) = score_search_candidate(
                query_zh,
                query_en,
                aliases,
                &item.title,
                &item.year,
                expected_year,
            );
            SearchCandidate {
                url: item.url.clone(),
                title: item.title.clone(),
                year: item.year.clone(),
                confidence,
                reason,
            }
        })
        .collect();

    // Sort by confidence descending
    candidates.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

    let best = candidates.first().cloned();

    if let Some(ref b) = best {
        eprintln!(
            "[Douban] selected: {} confidence={:.2} reason={}",
            b.title, b.confidence, b.reason
        );
    }

    Ok((best, candidates))
}

/// Fetch and parse Douban search results page.
async fn fetch_douban_search(query: &str) -> Result<Vec<DoubanSearchItem>, String> {
    let encoded: String = url::form_urlencoded::byte_serialize(query.as_bytes()).collect();
    let search_url = format!(
        "https://movie.douban.com/subject_search?search_text={}&cat=1002",
        encoded
    );
    eprintln!("[Douban] search URL: {}", search_url);

    let html = fetch_douban_with_challenge_solve(&search_url).await?;

    if is_login_page(&Html::parse_document(&html)) {
        eprintln!("[Douban] search: login page detected, skipping");
        return Ok(Vec::new());
    }

    parse_douban_search_results(&html)
}

/// Parse Douban search results HTML into structured items.
fn parse_douban_search_results(html: &str) -> Result<Vec<DoubanSearchItem>, String> {
    let document = Html::parse_document(html);

    let item_sel = Selector::parse(".item-root, .result, .search-result .item").unwrap();
    let link_sel = Selector::parse("a[href*='/subject/']").unwrap();
    let title_sel = Selector::parse(".title-text, .title a, h3 a, a.title").unwrap();
    let subject_re = Regex::new(r"/subject/(\d+)").map_err(|e| format!("regex error: {}", e))?;

    let mut items = Vec::new();

    // Strategy 1: iterate item containers
    for item_el in document.select(&item_sel) {
        let mut url = None;
        let mut title = None;

        // Find subject link
        for link in item_el.select(&link_sel) {
            let href = link.value().attr("href").unwrap_or("");
            if subject_re.is_match(href) && !href.contains("/celebrities") {
                url = Some(href.to_string());
                break;
            }
        }

        // Find title text
        for title_el in item_el.select(&title_sel) {
            let text = title_el.text().collect::<String>().trim().to_string();
            if !text.is_empty() {
                title = Some(text);
                break;
            }
        }

        // Fallback: use link text as title
        if title.is_none() {
            if let Some(ref u) = url {
                for link in item_el.select(&link_sel) {
                    let href = link.value().attr("href").unwrap_or("");
                    if href == u {
                        let text = link.text().collect::<String>().trim().to_string();
                        if !text.is_empty() {
                            title = Some(text);
                        }
                        break;
                    }
                }
            }
        }

        if let (Some(u), Some(t)) = (url, title) {
            // Extract year from nearby text (e.g., "(2025)")
            let item_text = item_el.text().collect::<String>();
            let year = extract_year_from_text(&item_text);
            items.push(DoubanSearchItem {
                url: ensure_absolute_url(&u),
                title: t,
                year,
            });
        }
    }

    // Strategy 2: if no items found, extract all /subject/ links directly
    if items.is_empty() {
        eprintln!("[Douban] search: fallback to direct /subject/ link extraction");
        for link in document.select(&link_sel) {
            let href = link.value().attr("href").unwrap_or("");
            if !href.contains("/celebrities") && subject_re.is_match(href) {
                let text = link.text().collect::<String>().trim().to_string();
                if !text.is_empty() && text.len() > 1 {
                    items.push(DoubanSearchItem {
                        url: ensure_absolute_url(href),
                        title: text,
                        year: None,
                    });
                }
            }
        }
    }

    // Deduplicate by URL
    let mut seen = std::collections::HashSet::new();
    items.retain(|item| seen.insert(item.url.clone()));

    Ok(items)
}

fn extract_year_from_text(text: &str) -> Option<String> {
    let re = Regex::new(r"\((\d{4})\)").ok()?;
    re.captures(text).map(|c| c[1].to_string())
}

fn ensure_absolute_url(url: &str) -> String {
    if url.starts_with("http") {
        url.to_string()
    } else if url.starts_with("//") {
        format!("https:{}", url)
    } else {
        format!("https://movie.douban.com{}", url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_login_page() {
        let doc = Html::parse_document("<html><body>请登录豆瓣</body></html>");
        assert!(is_login_page(&doc));
    }

    #[test]
    fn test_normal_page_not_login() {
        let doc = Html::parse_document("<html><body>Normal page content</body></html>");
        assert!(!is_login_page(&doc));
    }

    #[test]
    fn test_extract_from_empty_html() {
        let document = Html::parse_document("<html><body></body></html>");
        let chars = extract_cast_primary(&document);
        assert!(chars.is_empty());
    }

    #[test]
    fn test_parse_challenge_from_real_html() {
        let html = r#"<html><body>
            <form name="sec" id="sec" method="POST" action="/c">
              <input type="hidden" id="tok" name="tok" value="abc123" />
              <input type="hidden" id="cha" name="cha" value="deadbeef" />
              <input type="hidden" id="sol" name="sol" value="" />
              <input type="hidden" id="red" name="red" value="https://movie.douban.com/subject/36809858/celebrities">
            </form>
        </body></html>"#;
        let result = parse_challenge(html);
        assert!(result.is_some());
        let (tok, cha, red) = result.unwrap();
        assert_eq!(tok, "abc123");
        assert_eq!(cha, "deadbeef");
        assert!(red.contains("36809858"));
    }

    #[test]
    fn test_solve_challenge() {
        // solve_challenge should find a nonce for any input
        let nonce = solve_challenge("test_challenge");
        let hash = hex::encode(Sha512::digest(
            format!("test_challenge{}", nonce).as_bytes(),
        ));
        assert!(
            hash.starts_with("0000"),
            "hash should start with 0000, got {}",
            hash
        );
    }

    #[test]
    fn test_normalize_douban_url_full_trailing_slash() {
        let result =
            normalize_douban_celebrities_url("https://movie.douban.com/subject/36809858/").unwrap();
        assert_eq!(
            result,
            "https://movie.douban.com/subject/36809858/celebrities"
        );
    }

    #[test]
    fn test_normalize_douban_url_no_trailing_slash() {
        let result =
            normalize_douban_celebrities_url("https://movie.douban.com/subject/36809858").unwrap();
        assert_eq!(
            result,
            "https://movie.douban.com/subject/36809858/celebrities"
        );
    }

    #[test]
    fn test_normalize_douban_url_already_celebrities() {
        let result = normalize_douban_celebrities_url(
            "https://movie.douban.com/subject/36809858/celebrities",
        )
        .unwrap();
        assert_eq!(
            result,
            "https://movie.douban.com/subject/36809858/celebrities"
        );
    }

    #[test]
    fn test_normalize_douban_url_invalid() {
        let result = normalize_douban_celebrities_url("https://example.com/subject/123");
        assert!(result.is_err());
    }

    /// Diagnostic: fetch actual Douban page with challenge solving.
    /// cargo test scraper::douban::tests::diagnose_douban -- --nocapture --ignored
    #[tokio::test]
    #[ignore]
    async fn diagnose_douban() {
        let url = "https://movie.douban.com/subject/36809858/celebrities";
        match scrape_douban(url).await {
            Ok(result) => {
                println!("=== SUCCESS ===");
                println!("Drama title: {:?}", result.drama_title);
                println!("Page title: {:?}", result.page_title);
                println!("Characters: {} found", result.characters.len());
                for c in &result.characters {
                    println!("  actor={:?} role={}", c.actor_name, c.character_name);
                }
            }
            Err(e) => println!("=== FAILED: {} ===", e),
        }
    }
}
