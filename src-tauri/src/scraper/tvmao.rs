use scraper::{Html, Selector};

use super::{fetch_html, ScrapedCharacter, ScrapeResult, ScrapeSource};

/// Scrape TVMao (电视猫) drama cast page.
/// URL pattern: https://www.tvmao.com/drama/XXXXX
pub async fn scrape_tvmao(url: &str) -> Result<ScrapeResult, String> {
    let html = fetch_html(url).await?;
    let document = Html::parse_document(&html);

    // Extract page title
    let title_sel = Selector::parse("title").unwrap();
    let page_title = document
        .select(&title_sel)
        .next()
        .map(|e| e.text().collect::<String>().trim().to_string());

    // Extract drama title
    let drama_title = extract_drama_title(&document);

    // Extract synopsis
    let synopsis = extract_synopsis(&document);

    // Try primary selectors for cast
    let mut characters = extract_cast_primary(&document);
    if characters.is_empty() {
        characters = extract_cast_generic(&document);
    }

    if characters.is_empty() {
        // Try the alternate URL pattern
        return Err(format!(
            "电视猫: キャスト情報が見つかりませんでした。\nURL: {}\n\nヒント: TVMaoのキャストページに直接アクセスしてください。\nまたは、ページ本文を手動で貼り付けてください。",
            url
        ));
    }

    Ok(ScrapeResult {
        source: ScrapeSource::TvMao,
        url: url.to_string(),
        page_title,
        drama_title,
        synopsis,
        characters,
        saved_html_path: None,
    })
}

fn extract_drama_title(document: &Html) -> Option<String> {
    // Try heading
    let sel = Selector::parse("h1, .drama-title, .title").ok()?;
    document
        .select(&sel)
        .next()
        .map(|e| e.text().collect::<String>().trim().to_string())
}

fn extract_synopsis(document: &Html) -> Option<String> {
    let sel = Selector::parse(".drama-desc, .brief, .summary, .intro p").ok()?;
    document
        .select(&sel)
        .next()
        .map(|e| e.text().collect::<String>().trim().to_string())
}

/// Primary cast extraction for TVMao layout.
fn extract_cast_primary(document: &Html) -> Vec<ScrapedCharacter> {
    let row_sel = Selector::parse(".cast_item, .actor-item, .performer, li.actor, .role-list li")
        .unwrap();
    let name_sel = Selector::parse("a, .name, .actor-name").unwrap();
    let role_sel = Selector::parse(".role, .role-name, .char-name, em").unwrap();

    let mut chars = Vec::new();

    for (i, row) in document.select(&row_sel).enumerate() {
        // Character name (role) — the character they play
        let character_name = row
            .select(&role_sel)
            .next()
            .or_else(|| {
                // Sometimes role is right after actor name
                row.select(&name_sel).nth(1)
            })
            .map(|e| e.text().collect::<String>().trim().to_string());

        // Actor name
        let actor_name = row
            .select(&name_sel)
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

/// Generic fallback: find any structured cast-like sections.
fn extract_cast_generic(document: &Html) -> Vec<ScrapedCharacter> {
    // Try sections with "cast" or "actor" in class/id
    let section_sel =
        Selector::parse("[class*='cast'], [id*='cast'], [class*='actor'], [id*='actor']").unwrap();
    let link_sel = Selector::parse("a").unwrap();

    let mut chars = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for section in document.select(&section_sel) {
        let links: Vec<String> = section
            .select(&link_sel)
            .map(|e| e.text().collect::<String>().trim().to_string())
            .filter(|s| !s.is_empty() && s.len() > 1 && !s.starts_with("http"))
            .collect();

        // TVMao typically lists actor first, then character role
        for pair in links.chunks(2) {
            let actor = pair.first().cloned();
            let character = pair.get(1).cloned();
            if let Some(name) = character.or(actor) {
                let key = name.clone();
                if seen.insert(key) {
                    chars.push(ScrapedCharacter {
                        source_id: format!("cast_{:03}", chars.len()),
                        character_name: name,
                        actor_name: pair.first().cloned(),
                        role_type: None,
                        aliases: Vec::new(),
                    });
                }
            }
        }
    }
    chars
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
    fn test_extract_generic_from_empty() {
        let document = Html::parse_document("<html><body></body></html>");
        let chars = extract_cast_generic(&document);
        assert!(chars.is_empty());
    }
}
