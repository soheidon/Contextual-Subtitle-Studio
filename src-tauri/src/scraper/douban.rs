use scraper::{Html, Selector};
use sha2::{Sha512, Digest};

use super::{ScrapedCharacter, ScrapeResult, ScrapeSource};

const USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/125.0.0.0 Safari/537.36";

/// Scrape 豆瓣 movie/drama page via celebrities page.
/// URL pattern: https://movie.douban.com/subject/XXXXX/celebrities
///
/// Douban uses a SHA-512 proof-of-work challenge for automated requests.
/// This scraper solves the challenge in Rust to obtain a session cookie,
/// then fetches the actual page content.
pub async fn scrape_douban(url: &str) -> Result<ScrapeResult, String> {
    let html = fetch_douban_with_challenge_solve(url).await?;
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
    if characters.is_empty() {
        characters = extract_cast_celebrities(&document);
    }
    if characters.is_empty() {
        characters = extract_cast_generic(&document);
    }

    if characters.is_empty() {
        return Err(format!(
            "豆瓣: キャスト情報が見つかりませんでした。\nURL: {}\nページタイトル: {:?}",
            url, page_title
        ));
    }

    Ok(ScrapeResult {
        source: ScrapeSource::Douban,
        url: url.to_string(),
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
                "zh-CN,zh;q=0.9,en;q=0.8"
                    .parse()
                    .unwrap(),
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
async fn submit_challenge(
    tok: &str,
    cha: &str,
    nonce: u64,
    red: &str,
) -> Result<String, String> {
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
        return Err(format!("Expected 302 after challenge, got {}", resp.status()));
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
    let row_sel =
        Selector::parse("#celebrities .celebrity, .casting_list li, ul.celebrity-list li, ul.celebrities-list li")
            .unwrap();
    let name_sel = Selector::parse(".name a, a[rel='v:starring']").unwrap();
    let role_sel = Selector::parse(".role, em, .character").unwrap();

    let mut chars = Vec::new();
    for (i, row) in document.select(&row_sel).enumerate() {
        let raw_actor = row
            .select(&name_sel)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string());

        let raw_role = row.select(&role_sel).next().map(|e| {
            e.text().collect::<String>().trim().to_string()
        });

        // Parse actor name: "黄杨钿甜 Tiantian Huangyang" → (cn="黄杨钿甜", en="Tiantian Huangyang")
        let (actor_cn, actor_en) = raw_actor
            .as_deref()
            .map(split_cn_en_name)
            .unwrap_or((None, None));

        // Parse role: "演员 Actress (饰 楚乔)" → "楚乔", skip non-actors
        let role_type = raw_role.as_deref().map(classify_douban_role);
        let is_actor = matches!(role_type, Some(DoubanRoleType::Actor));
        let character_name = raw_role
            .as_deref()
            .and_then(extract_douban_character_name);

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
    let cn = chars[..cn_end].iter().collect::<String>().trim().to_string();
    let en = chars[cn_end..].iter().collect::<String>().trim().to_string();
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
        let hash = hex::encode(Sha512::digest(format!("test_challenge{}", nonce).as_bytes()));
        assert!(hash.starts_with("0000"), "hash should start with 0000, got {}", hash);
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
