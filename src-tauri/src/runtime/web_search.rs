//! Bounded web search through DuckDuckGo HTML.
//! No shell, Agent-supplied URL, command or external search service.

use std::time::Duration;

use reqwest::blocking::Client;
use scraper::{Html, Selector};
use serde_json::{json, Value};
use url::Url;

use super::skill_invocation::{
    SkillInvocationError, SkillInvocationErrorKind,
    SkillInvocationRequest, SkillInvocationResult,
};

#[derive(Debug, PartialEq, Eq)]
struct SearchHit {
    title: String,
    url: String,
    snippet: String,
}

fn invalid() -> SkillInvocationError {
    SkillInvocationError::new(
        SkillInvocationErrorKind::InvalidRequest,
        "Web search requires query (1–200 characters) and optional maxResults (1–10).",
        false,
    )
}

fn failed() -> SkillInvocationError {
    SkillInvocationError::new(
        SkillInvocationErrorKind::ExecutionFailed,
        "Web search failed or returned no usable results.",
        true,
    )
}

fn validate(input: &Value) -> Result<(&str, usize), SkillInvocationError> {
    let object = input.as_object().ok_or_else(invalid)?;

    if object
        .keys()
        .any(|key| key != "query" && key != "maxResults")
    {
        return Err(invalid());
    }

    let query = object
        .get("query")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?;

    if query.trim().is_empty()
        || query.chars().count() > 200
        || query.chars().any(char::is_control)
    {
        return Err(invalid());
    }

    let max_results = match object.get("maxResults") {
        None => 5,
        Some(value) => {
            let n = value.as_u64().ok_or_else(invalid)?;
            if !(1..=10).contains(&n) {
                return Err(invalid());
            }
            n as usize
        }
    };

    Ok((query, max_results))
}

fn target_url(href: &str) -> Option<String> {
    // DuckDuckGo may return redirects using /l/?uddg=<encoded URL>.
    let parsed = Url::parse(href)
        .or_else(|_| {
            Url::parse("https://html.duckduckgo.com")
                .and_then(|base| base.join(href))
        })
        .ok()?;

    let candidate = if parsed
        .host_str()
        .is_some_and(|host| host == "duckduckgo.com"
            || host.ends_with(".duckduckgo.com"))
        && parsed.path().starts_with("/l/")
    {
        parsed
            .query_pairs()
            .find(|(key, _)| key == "uddg")
            .map(|(_, value)| value.into_owned())?
    } else {
        parsed.to_string()
    };

    let target = Url::parse(&candidate).ok()?;

    if !matches!(target.scheme(), "http" | "https") {
        return None;
    }

    let host = target.host_str()?;

    if host.is_empty()
        || host == "localhost"
        || host.ends_with(".localhost")
        || host == "127.0.0.1"
        || host == "::1"
    {
        return None;
    }

    Some(target.to_string())
}

fn parse_results(html: &str, max_results: usize) -> Vec<SearchHit> {
    let document = Html::parse_document(html);

    let container = Selector::parse(".result").expect("static CSS selector");
    let title = Selector::parse("a.result__a").expect("static CSS selector");
    let snippet = Selector::parse(".result__snippet").expect("static CSS selector");

    let mut results = Vec::new();

    for element in document.select(&container) {
        let Some(link) = element.select(&title).next() else {
            continue;
        };

        let Some(href) = link.value().attr("href") else {
            continue;
        };

        let Some(url) = target_url(href) else {
            continue;
        };

        let title_text = link.text().collect::<Vec<_>>().join(" ");
        let title_text = title_text.trim();

        if title_text.is_empty() {
            continue;
        }

        let snippet_text = element
            .select(&snippet)
            .next()
            .map(|node| node.text().collect::<Vec<_>>().join(" "))
            .unwrap_or_default();

        if results.iter().any(|item: &SearchHit| item.url == url) {
            continue;
        }

        results.push(SearchHit {
            title: title_text.to_owned(),
            url,
            snippet: snippet_text.trim().to_owned(),
        });

        if results.len() >= max_results {
            break;
        }
    }

    results
}

pub(super) fn search(
    request: &SkillInvocationRequest,
) -> Result<SkillInvocationResult, SkillInvocationError> {
    if request.capability != "browser.search" {
        return Err(invalid());
    }

    let (query, max_results) = validate(&request.input)?;

    let client = Client::builder()
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(5))
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)")
        .redirect(reqwest::redirect::Policy::limited(3))
        .build()
        .map_err(|_| failed())?;

    // URL is fixed; query goes through the HTTP client's URL encoder.
    let mut search_url =
        Url::parse("https://html.duckduckgo.com/html/")
            .map_err(|_| failed())?;

    search_url.query_pairs_mut().append_pair("q", query);

    let response = client
        .get(search_url)
        .send()
        .map_err(|_| failed())?;

    if !response.status().is_success() {
        return Err(failed());
    }

    // Avoid reading arbitrarily large responses.
    if response.content_length().is_some_and(|size| size > 1_000_000) {
        return Err(failed());
    }

    let bytes = response.bytes().map_err(|_| failed())?;

    if bytes.len() > 1_000_000 {
        return Err(failed());
    }

    let html = String::from_utf8_lossy(&bytes);
    let results = parse_results(&html, max_results);

    if results.is_empty() {
        return Err(failed());
    }

    let output = results
        .into_iter()
        .map(|hit| {
            json!({
                "title": hit.title,
                "url": hit.url,
                "snippet": hit.snippet
            })
        })
        .collect::<Vec<Value>>();

    Ok(SkillInvocationResult {
        invocation_id: request.invocation_id.clone(),
        capability: request.capability.clone(),
        backend: "web-search".to_owned(),
        provider: Some("duckduckgo-html".to_owned()),
        output: json!({
            "query": query,
            "provider": "duckduckgo-html",
            "results": output
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_bounded_search_requests() {
        assert_eq!(validate(&json!({"query": "AI-OS"})).unwrap().1, 5);
        assert_eq!(
            validate(&json!({"query": "AI-OS", "maxResults": 10}))
                .unwrap()
                .1,
            10
        );
    }

    #[test]
    fn rejects_unsafe_search_requests() {
        for input in [
            json!({}),
            json!({"query": ""}),
            json!({"query": "  "}),
            json!({"query": "x".repeat(201)}),
            json!({"query": "abc\nxyz"}),
            json!({"query": "AI", "command": "sh"}),
            json!({"query": "AI", "args": ["-c"]}),
            json!({"query": "AI", "url": "http://localhost"}),
            json!({"query": "AI", "maxResults": 0}),
            json!({"query": "AI", "maxResults": 11}),
        ] {
            assert!(validate(&input).is_err(), "{input:?}");
        }
    }

    #[test]
    fn extracts_titles_snippets_and_redirect_targets() {
        let html = r#"
        <div class="result">
          <a class="result__a"
             href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com%2Farticle">
             Example article
          </a>
          <a class="result__snippet">Useful search summary</a>
        </div>
        "#;

        let results = parse_results(html, 5);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Example article");
        assert_eq!(results[0].url, "https://example.com/article");
        assert_eq!(results[0].snippet, "Useful search summary");
    }

    #[test]
    fn rejects_invalid_result_links() {
        assert!(target_url("javascript:alert(1)").is_none());
        assert!(target_url("http://localhost/test").is_none());
        assert!(target_url("http://127.0.0.1/test").is_none());
    }

    #[test]
    #[ignore = "Requires internet access"]
    fn real_duckduckgo_search_returns_results() {
        let request = SkillInvocationRequest::new(
            "search-real-test",
            "browser.search",
            json!({"query": "AI-OS", "maxResults": 5}),
        )
        .unwrap();

        let result = search(&request).unwrap();

        assert_eq!(result.provider.as_deref(), Some("duckduckgo-html"));

        let results = result.output["results"].as_array().unwrap();

        assert!(!results.is_empty());
        assert!(results.len() <= 5);

        for item in results {
            assert!(item["title"].as_str().is_some_and(|s| !s.is_empty()));
            assert!(item["url"].as_str().unwrap().starts_with("http"));
        }
    }
}
