use crate::scraper::fetch_html;
use scraper::{Html, Selector};
use tokio::time::{sleep, timeout, Duration};

#[derive(Debug, Clone)]
pub struct SearchSnippet {
    pub title: String,
    pub snippet: String,
    pub url: String,
}

fn encode_query(s: &str) -> String {
    let mut encoded = String::with_capacity(s.len() * 3);
    for byte in s.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char);
            }
            b' ' => {
                encoded.push('+');
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    encoded
}

fn parse_ddg_results(document: &Html, max_results: usize) -> Vec<SearchSnippet> {
    let result_sel = Selector::parse(".result").unwrap();
    let title_sel = Selector::parse(".result__a, .result__title").unwrap();
    let snippet_sel = Selector::parse(".result__snippet").unwrap();
    let url_sel = Selector::parse(".result__url").unwrap();

    let mut results = Vec::new();
    for result_el in document.select(&result_sel).take(max_results) {
        let title = result_el
            .select(&title_sel)
            .next()
            .map(|el| el.text().collect::<Vec<_>>().join("").trim().to_string())
            .unwrap_or_default();
        let snippet = result_el
            .select(&snippet_sel)
            .next()
            .map(|el| el.text().collect::<Vec<_>>().join("").trim().to_string())
            .unwrap_or_default();
        let url = result_el
            .select(&url_sel)
            .next()
            .map(|el| el.text().collect::<Vec<_>>().join("").trim().to_string())
            .unwrap_or_default();

        if !title.is_empty() || !snippet.is_empty() {
            results.push(SearchSnippet {
                title,
                snippet,
                url,
            });
        }
    }
    results
}

pub async fn search_duckduckgo(
    query: &str,
    max_results: usize,
) -> Result<Vec<SearchSnippet>, String> {
    let url = format!(
        "https://html.duckduckgo.com/html/?q={}",
        encode_query(query)
    );
    let html = fetch_html(&url).await?;
    Ok(parse_ddg_results(&Html::parse_document(&html), max_results))
}

pub async fn search_drama_context(title_zh: &str, title_en: &str, year: Option<&str>) -> String {
    let mut all_snippets: Vec<String> = Vec::new();

    let year_suffix = year.map(|y| format!(" {}", y)).unwrap_or_default();

    // Search 1: Chinese title + drama keywords
    if !title_zh.is_empty() {
        let q1 = format!("{} 电视剧 剧情 角色 世界观{}", title_zh, year_suffix);
        let result = timeout(Duration::from_secs(10), search_duckduckgo(&q1, 5)).await;
        match result {
            Ok(Ok(results)) => {
                for r in &results {
                    all_snippets.push(format!("- [{}]({})\n  {}", r.title, r.url, r.snippet));
                }
            }
            _ => {}
        }
        sleep(Duration::from_millis(800)).await;
    }

    // Search 2: English title + drama keywords
    if !title_en.is_empty() {
        let q2 = format!("{} drama plot characters setting{}", title_en, year_suffix);
        let result = timeout(Duration::from_secs(10), search_duckduckgo(&q2, 5)).await;
        match result {
            Ok(Ok(results)) => {
                for r in &results {
                    all_snippets.push(format!("- [{}]({})\n  {}", r.title, r.url, r.snippet));
                }
            }
            _ => {}
        }
        sleep(Duration::from_millis(800)).await;

        // Search 3: English title + analytical keywords
        let q3 = format!("{} drama review analysis plot summary", title_en);
        let result = timeout(Duration::from_secs(10), search_duckduckgo(&q3, 5)).await;
        match result {
            Ok(Ok(results)) => {
                for r in &results {
                    all_snippets.push(format!("- [{}]({})\n  {}", r.title, r.url, r.snippet));
                }
            }
            _ => {}
        }
    }

    if all_snippets.is_empty() {
        return String::new();
    }

    format!(
        "【Web検索結果】\n以下はウェブ検索で見つかった作品に関する外部情報です。\n\n{}",
        all_snippets.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_query_ascii() {
        let encoded = encode_query("hello world");
        assert_eq!(encoded, "hello+world");
    }

    #[test]
    fn test_encode_query_chinese() {
        let encoded = encode_query("冰湖");
        // 冰 = E5 86 B0, 湖 = E6 B9 96
        assert_eq!(encoded, "%E5%86%B0%E6%B9%96");
    }

    #[test]
    fn test_encode_query_mixed() {
        let encoded = encode_query("冰湖 2024");
        assert!(encoded.contains("%E5%86%B0%E6%B9%96"));
        assert!(encoded.contains("+2024"));
    }

    #[test]
    fn test_parse_ddg_results_empty() {
        let html = "<html><body></body></html>";
        let doc = Html::parse_document(html);
        let results = parse_ddg_results(&doc, 5);
        assert!(results.is_empty());
    }

    #[test]
    fn test_parse_ddg_results_with_results() {
        let html = r#"
        <html><body>
        <div class="result">
            <a class="result__a">Test Drama (2025)</a>
            <span class="result__snippet">A wonderful drama about love and revenge.</span>
            <span class="result__url">mydramalist.com</span>
        </div>
        <div class="result">
            <a class="result__a">Test Drama Review</a>
            <span class="result__snippet">Detailed analysis of the plot.</span>
            <span class="result__url">example.com</span>
        </div>
        </body></html>
        "#;
        let doc = Html::parse_document(html);
        let results = parse_ddg_results(&doc, 5);
        assert_eq!(results.len(), 2);
        assert!(results[0].title.contains("Test Drama"));
        assert!(results[0].snippet.contains("wonderful drama"));
        assert_eq!(results[0].url, "mydramalist.com");
        assert!(results[1].title.contains("Review"));
        assert!(results[1].snippet.contains("analysis"));
    }

    #[test]
    fn test_parse_ddg_results_respects_max() {
        let mut html = String::from("<html><body>");
        for i in 0..10 {
            html.push_str(&format!(
                r#"<div class="result">
                <a class="result__a">Result {}</a>
                <span class="result__snippet">Snippet {}</span>
                <span class="result__url">url{}.com</span>
                </div>"#,
                i, i, i
            ));
        }
        html.push_str("</body></html>");
        let doc = Html::parse_document(&html);
        let results = parse_ddg_results(&doc, 3);
        assert_eq!(results.len(), 3);
    }
}
