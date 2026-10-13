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

    let mut target = Url::parse(&candidate).ok()?;

    if !matches!(target.scheme(), "http" | "https") {
        return None;
    }

    let host = target.host_str()?.to_owned();

    if host.is_empty()
        || host == "localhost"
        || host.ends_with(".localhost")
        || host == "127.0.0.1"
        || host == "::1"
    {
        return None;
    }

    // A decoded target that still points at DuckDuckGo is its own ad or
    // click-tracking redirect (y.js?ad_domain=...), never a destination.
    if host == "duckduckgo.com" || host.ends_with(".duckduckgo.com") {
        return None;
    }

    // Show the canonical destination: drop click-tracking parameters.
    let kept: Vec<(String, String)> = target
        .query_pairs()
        .filter(|(key, _)| !is_tracking_parameter(key))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    if kept.is_empty() {
        target.set_query(None);
    } else {
        target.query_pairs_mut().clear().extend_pairs(kept);
    }
    target.set_fragment(None);

    let url = target.to_string();
    if url.len() > MAX_RESULT_URL_LENGTH {
        return None;
    }
    Some(url)
}

const MAX_RESULT_URL_LENGTH: usize = 300;

fn is_tracking_parameter(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.starts_with("utm_")
        || matches!(
            key.as_str(),
            "gclid" | "gclsrc" | "dclid" | "fbclid" | "msclkid" | "yclid"
                | "mc_cid" | "mc_eid" | "_hsenc" | "_hsmi" | "srsltid"
        )
}

/// Model-like query terms ("ds925+", "ts-464") reduced to letters and digits.
fn identifier_terms(query: &str) -> Vec<String> {
    query
        .split_whitespace()
        .map(compact)
        .filter(|term| {
            term.len() >= 3
                && term.chars().any(|c| c.is_ascii_digit())
                && term.chars().any(|c| c.is_ascii_alphabetic())
        })
        .collect()
}

fn compact(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn parse_results(html: &str, query: &str, max_results: usize) -> Vec<SearchHit> {
    let document = Html::parse_document(html);

    let container = Selector::parse(".result").expect("static CSS selector");
    let title = Selector::parse("a.result__a").expect("static CSS selector");
    let snippet = Selector::parse(".result__snippet").expect("static CSS selector");
    let identifiers = identifier_terms(query);

    let mut results = Vec::new();

    for element in document.select(&container) {
        // Sponsored results are advertising, not evidence.
        if element.value().classes().any(|class| class.starts_with("result--ad")) {
            continue;
        }

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

        // When the query names a specific model, a result that mentions it
        // nowhere is unrelated to the question.
        if !identifiers.is_empty() {
            let haystack = compact(&format!("{title_text} {snippet_text} {url}"));
            if !identifiers.iter().any(|term| haystack.contains(term.as_str())) {
                continue;
            }
        }

        results.push(SearchHit {
            title: title_text.to_owned(),
            url,
            snippet: snippet_text.trim().to_owned(),
        });

        // Read past max_results: source selection below needs every candidate.
        if results.len() >= MAX_PARSED_RESULTS {
            break;
        }
    }

    select_sources(results, max_results)
}

const MAX_PARSED_RESULTS: usize = 20;

/// Second-level labels that sit under a country code ("example.com.au").
const COUNTRY_SECOND_LEVEL: &[&str] = &["com", "co", "net", "org", "gov", "edu", "ac", "or", "ne"];

/// Approximate registrable domain and its name label, without a suffix list:
/// "kb.synology.com" -> ("synology.com", "synology"),
/// "www.synology.com.au" -> ("synology.com.au", "synology"),
/// "synology.cn.com" -> ("cn.com", "cn").
fn registrable(host: &str) -> Option<(String, String)> {
    let labels: Vec<&str> = host.trim_end_matches('.').split('.').collect();
    if labels.len() < 2 {
        return None;
    }
    let tld = labels[labels.len() - 1];
    let second = labels[labels.len() - 2];
    let take = if labels.len() >= 3 && tld.len() == 2 && COUNTRY_SECOND_LEVEL.contains(&second) {
        3
    } else {
        2
    };
    let domain = labels[labels.len() - take..].join(".");
    Some((domain, labels[labels.len() - take].to_owned()))
}

/// Official-source selection, using only the results themselves:
/// - a host that carries another result's site name as a subdomain label under
///   a different registrable domain ("synology.cn.com" beside "synology.com")
///   is a look-alike, not the manufacturer, and is dropped;
/// - sites that answer with several pages rank before one-off pages, keeping
///   DuckDuckGo's order otherwise.
fn select_sources(results: Vec<SearchHit>, max_results: usize) -> Vec<SearchHit> {
    let sites: Vec<Option<(String, String)>> = results
        .iter()
        .map(|hit| Url::parse(&hit.url).ok()?.host_str().and_then(registrable))
        .collect();
    let names: std::collections::HashSet<&str> =
        sites.iter().flatten().map(|(_, name)| name.as_str()).collect();

    let mut kept: Vec<(usize, &SearchHit, usize)> = Vec::new();
    for (index, hit) in results.iter().enumerate() {
        let Some((domain, _)) = &sites[index] else { continue };
        let host = Url::parse(&hit.url).ok().and_then(|u| u.host_str().map(str::to_owned)).unwrap_or_default();
        let prefix = host.strip_suffix(domain.as_str()).unwrap_or("").trim_end_matches('.');
        let look_alike = prefix
            .split('.')
            .filter(|label| !label.is_empty())
            .any(|label| names.contains(label) && registrable_name(domain) != label);
        if look_alike {
            continue;
        }
        let pages = sites.iter().flatten().filter(|(other, _)| other == domain).count();
        kept.push((index, hit, pages));
    }
    kept.sort_by(|a, b| b.2.cmp(&a.2).then(a.0.cmp(&b.0)));
    kept.into_iter()
        .take(max_results)
        .map(|(_, hit, _)| SearchHit { title: hit.title.clone(), url: hit.url.clone(), snippet: hit.snippet.clone() })
        .collect()
}

fn registrable_name(domain: &str) -> &str {
    domain.split('.').next().unwrap_or("")
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
    let results = parse_results(&html, query, max_results);

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

        let results = parse_results(html, "example article", 5);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].title, "Example article");
        assert_eq!(results[0].url, "https://example.com/article");
        assert_eq!(results[0].snippet, "Useful search summary");
    }

    #[test]
    fn excludes_ads_tracking_redirects_and_unrelated_results() {
        // Shape taken from a real html.duckduckgo.com response for
        // "nas ds925+ official product page" on 2026-10-13.
        let html = r#"
        <div class="result results_links results_links_deep result--ad">
          <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fduckduckgo.com%2Fy.js%3Fad_domain%3Debay.com.au%26ad_provider%3Dbingv7aa">
            Huge Selections &amp; Great Prices - Nas Storage Devices On eBay</a>
          <a class="result__snippet">Browse Nas storage devices! Find the deal you deserve on eBay.</a>
        </div>
        <div class="result results_links results_links_deep web-result">
          <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fduckduckgo.com%2Fy.js%3Fad_domain%3Dexample.com">
            Unmarked tracking redirect DS925+</a>
        </div>
        <div class="result results_links results_links_deep web-result">
          <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fwww.synology.com%2Fen%2Dglobal%2Fproducts%2FDS925%2B%3Futm_source%3Dddg%26lang%3Den&amp;rut=bf22">
            DiskStation DS925+ | Synology Inc.</a>
          <a class="result__snippet">Synology DS925+ 4-bay NAS.</a>
        </div>
        <div class="result results_links results_links_deep web-result">
          <a class="result__a" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.org%2Fbest-nas">
            Best NAS for small business</a>
          <a class="result__snippet">A roundup of storage devices.</a>
        </div>
        "#;

        let results = parse_results(html, "nas ds925+ official product page", 5);

        assert_eq!(results.len(), 1, "{results:?}");
        assert_eq!(results[0].title, "DiskStation DS925+ | Synology Inc.");
        assert_eq!(
            results[0].url,
            "https://www.synology.com/en-global/products/DS925+?lang=en"
        );
    }

    #[test]
    fn drops_look_alike_domains_and_ranks_the_manufacturer_first() {
        // Observed in the installed-app acceptance on 2026-10-13: a
        // look-alike "synology.cn.com" page was listed as the first source.
        let hit = |title: &str, url: &str| SearchHit {
            title: title.to_owned(),
            url: url.to_owned(),
            snippet: "DS925+".to_owned(),
        };
        let results = select_sources(
            vec![
                hit("DS925+ | Synology 群晖4盘位机旗舰机", "https://www.synology.cn.com/products/nas/ds925plus"),
                hit("DS925+ review", "https://example.org/ds925plus"),
                hit("DiskStation DS925+ | Synology Inc.", "https://www.synology.com/en-me/products/DS925+"),
                hit("群晖科技 DS925+", "https://www.synology.cn/zh-cn/products/DS925+"),
                hit("DiskStation® DS925+ | 群暉科技 Synology Inc.", "https://www.synology.com/zh-tw/products/DS925+"),
                hit("Synology Australia DS925+", "https://www.synology.com.au/ds925"),
            ],
            5,
        );
        let urls: Vec<&str> = results.iter().map(|hit| hit.url.as_str()).collect();
        assert_eq!(
            urls,
            vec![
                "https://www.synology.com/en-me/products/DS925+",
                "https://www.synology.com/zh-tw/products/DS925+",
                "https://example.org/ds925plus",
                "https://www.synology.cn/zh-cn/products/DS925+",
                "https://www.synology.com.au/ds925",
            ]
        );
        assert_eq!(registrable("kb.synology.com"), Some(("synology.com".to_owned(), "synology".to_owned())));
        assert_eq!(registrable("www.synology.com.au"), Some(("synology.com.au".to_owned(), "synology".to_owned())));
        assert_eq!(registrable("www.synology.cn.com"), Some(("cn.com".to_owned(), "cn".to_owned())));
    }

    #[test]
    fn drops_oversized_urls_and_keeps_queries_without_identifiers() {
        let long = format!("https://example.com/{}", "a".repeat(400));
        assert!(target_url(&long).is_none());
        assert_eq!(identifier_terms("what is raid"), Vec::<String>::new());
        assert_eq!(identifier_terms("nas ds925+ official"), vec!["ds925".to_owned()]);
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
