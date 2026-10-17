use crate::cloud_drive_core::{
    CloudDriveProvider, DownloadDescriptor, DownloadHeader, ShareEntry, ShareEntryKind, ShareLink,
};
use reqwest::{
    blocking::Client,
    cookie::{CookieStore, Jar},
    header::{COOKIE, REFERER, USER_AGENT},
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};
use url::Url;

const BAIDU_USER_AGENT: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 Chrome/140 Safari/537.36";
const BAIDU_APP_ID: &str = "250528";
/// Client identities tried in order when Baidu's file servers refuse a
/// download with 403: the desktop client, the web API identity, a browser.
pub const BAIDU_DOWNLOAD_USER_AGENTS: &[&str] = &[
    "netdisk;P2SP;3.0.0.8",
    "pan.baidu.com",
    BAIDU_USER_AGENT,
];
const MAX_SHARE_ITEMS: usize = 10_000;
const BAIDU_AUTH_COOKIE_NAMES: &[&str] = &[
    "BAIDUID",
    "BAIDUID_BFESS",
    "BDUSS",
    "BDUSS_BFESS",
    "PANWEB",
    "STOKEN",
];

pub fn is_authorization_cookie_name(name: &str) -> bool {
    BAIDU_AUTH_COOKIE_NAMES.contains(&name)
}

pub struct BaiduResolvedShare {
    client: Client,
    share: ShareLink,
    randsk: String,
    cookie_header: String,
    /// Cookie names (never values) seen at each step, for diagnosing a lost
    /// sign-in.
    cookie_trace: String,
    share_id: u64,
    share_uk: u64,
    sign: Option<String>,
    timestamp: Option<u64>,
    entries: Vec<ShareEntry>,
    files: HashMap<String, BaiduFile>,
    seen_items: HashSet<String>,
}

#[derive(Clone)]
struct BaiduFile {
    fs_id: u64,
    name: String,
    size: u64,
}

#[derive(Debug, Deserialize)]
struct BaiduVerifyResponse {
    errno: i64,
    #[serde(default)]
    randsk: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BaiduListResponse {
    errno: i64,
    #[serde(default)]
    list: Vec<BaiduRawItem>,
    #[serde(default)]
    has_more: bool,
}

#[derive(Debug, Clone, Deserialize)]
struct BaiduRawItem {
    fs_id: u64,
    #[serde(alias = "filename", alias = "name")]
    server_filename: String,
    path: String,
    isdir: u8,
    #[serde(default)]
    size: u64,
}

#[derive(Debug, Deserialize)]
struct BaiduDownloadResponse {
    errno: i64,
    #[serde(default)]
    list: Vec<BaiduDownloadItem>,
}

#[derive(Debug, Deserialize)]
struct BaiduDownloadItem {
    dlink: String,
}

impl BaiduResolvedShare {
    pub fn resolve(share: ShareLink) -> Result<Self, String> {
        Self::resolve_with_authorization(share, None)
    }

    pub fn resolve_with_authorization(
        share: ShareLink,
        authorization_cookie: Option<&str>,
    ) -> Result<Self, String> {
        if share.provider != CloudDriveProvider::Baidu {
            return Err("Baidu resolver received a different provider.".to_owned());
        }
        let authorization_cookie = authorization_cookie
            .map(validate_authorization_cookie)
            .transpose()?;
        let extraction_code = share
            .extraction_code
            .as_deref()
            .ok_or_else(|| "Baidu share extraction code is required.".to_owned())?;
        // A browser-grade cookie jar (RFC 6265 domain/path/secure rules).
        // Signed-in share pages redirect through Baidu hosts that set cookies
        // on the way; without a jar the redirects looped ("error following
        // redirect"), and a hand-written jar made Baidu serve a page without
        // its metadata (owner acceptance 2026-10-17).
        let jar = Arc::new(Jar::default());
        let pan = Url::parse("https://pan.baidu.com/").expect("static URL");
        if let Some(cookie) = authorization_cookie.as_deref() {
            for pair in cookie.split(';').map(str::trim).filter(|pair| !pair.is_empty()) {
                jar.add_cookie_str(&format!("{pair}; Domain=.baidu.com; Path=/; Secure"), &pan);
            }
        }
        let client = Client::builder()
            .user_agent(BAIDU_USER_AGENT)
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(60))
            .cookie_provider(jar.clone())
            .build()
            .map_err(|error| format!("Unable to create Baidu share client: {error}"))?;
        let init_url = format!(
            "https://pan.baidu.com/share/init?surl={}",
            baidu_surl(&share)?
        );
        let names_at = |stage: &str| -> String {
            let names = jar
                .cookies(&pan)
                .and_then(|value| value.to_str().ok().map(str::to_owned))
                .unwrap_or_default()
                .split(';')
                .filter_map(|pair| pair.split_once('=').map(|(name, _)| name.trim().to_owned()))
                .filter(|name| is_authorization_cookie_name(name))
                .collect::<Vec<_>>()
                .join(",");
            format!("{stage}[{names}]")
        };
        let mut cookie_trace = vec![names_at("received")];
        let init_response = client
            .get(&share.canonical_url)
            .send()
            .map_err(|error| format!("Unable to open Baidu share: {error}"))?;
        if !init_response.status().is_success() {
            return Err(format!(
                "Baidu share page returned HTTP {}.",
                init_response.status()
            ));
        }

        let verify_url = baidu_api_url("https://pan.baidu.com/share/verify", &share)?;
        let mut verify_request = client
            .post(verify_url)
            .header(REFERER, &init_url)
            .form(&[("pwd", extraction_code), ("vcode", ""), ("vcode_str", "")]);
        cookie_trace.push(names_at("opened"));
        let verify_response = verify_request
            .send()
            .map_err(|error| format!("Unable to verify Baidu extraction code: {error}"))?;
        if !verify_response.status().is_success() {
            return Err(format!(
                "Baidu extraction-code verification returned HTTP {}.",
                verify_response.status()
            ));
        }
        let verified: BaiduVerifyResponse = verify_response
            .json()
            .map_err(|_| "Baidu extraction-code response was invalid.".to_owned())?;
        if verified.errno != 0 {
            return Err(baidu_error("extraction-code verification", verified.errno));
        }
        let randsk = verified
            .randsk
            .filter(|value| {
                !value.is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
            })
            .ok_or_else(|| {
                "Baidu extraction-code response omitted its session proof.".to_owned()
            })?;
        // The verify response set BDCLND in the jar, as it would in a browser.
        cookie_trace.push(names_at("verified"));
        let share_response = client
            .get(&share.canonical_url)
            .header(REFERER, &init_url)
            .send()
            .map_err(|error| format!("Unable to open verified Baidu share: {error}"))?;
        // Later API and file requests carry exactly what the jar holds for pan.
        cookie_trace.push(names_at("page"));
        // The user's sign-in cookies are always sent as read from the sign-in
        // window, even if a response removed them from the jar: owner
        // acceptance showed BDUSS missing from the download request.
        let jar_header = jar
            .cookies(&pan)
            .and_then(|value| value.to_str().ok().map(str::to_owned))
            .unwrap_or_else(|| format!("BDCLND={randsk}"));
        let cookie_header = merge_cookie_headers(&jar_header, authorization_cookie.as_deref().unwrap_or(""));
        let cookie_trace = cookie_trace.join(" → ");
        if !share_response.status().is_success() {
            return Err(format!(
                "Verified Baidu share returned HTTP {}.",
                share_response.status()
            ));
        }
        let html = share_response
            .text()
            .map_err(|_| "Unable to read verified Baidu share page.".to_owned())?;
        let page = extract_locals_json(&html).map_err(|error| {
            format!(
                "{error} (bytes={}, locals={}, yunData={}, fileList={})",
                html.len(),
                html.contains("locals.mset("),
                html.contains("window.yunData"),
                html.contains("file_list")
            )
        })?;
        let share_id = json_u64(&page, "shareid")?;
        let share_uk = json_u64(&page, "share_uk")?;
        let timestamp = json_u64(&page, "timestamp").ok();
        let sign = page_value(&page, "sign")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty() && value.len() <= 4096)
            .map(str::to_owned);
        let root_items = page_items(&page)?;
        let mut resolved = Self {
            client,
            share,
            randsk,
            cookie_header,
            cookie_trace,
            share_id,
            share_uk,
            sign,
            timestamp,
            entries: Vec::new(),
            files: HashMap::new(),
            seen_items: HashSet::new(),
        };
        resolved.entries = resolved.build_entries(root_items)?;
        if resolved.entries.is_empty() {
            return Err("Baidu share did not contain any items.".to_owned());
        }
        Ok(resolved)
    }

    pub fn entries(&self) -> &[ShareEntry] {
        &self.entries
    }

    pub fn descriptor(&self, item_id: &str) -> Result<DownloadDescriptor, String> {
        self.descriptor_as(item_id, BAIDU_USER_AGENT)
    }

    /// Resolves the file as a given client identity. Baidu may bind the
    /// download URL to the identity that requested it, so the same
    /// User-Agent is used to request the URL and to download it.
    pub fn descriptor_as(
        &self,
        item_id: &str,
        user_agent: &str,
    ) -> Result<DownloadDescriptor, String> {
        let file = self
            .files
            .get(item_id)
            .ok_or_else(|| "Selected Baidu file is stale or is a folder.".to_owned())?;
        // Current share pages no longer embed sign/timestamp; the web client
        // asks tplconfig for them, so does AI-OS (verified 2026-10-17). A
        // fresh pair is fetched per file because signatures expire.
        let (sign, timestamp) = match self.fetch_download_signature() {
            Ok(pair) => pair,
            Err(error) => match (self.sign.clone(), self.timestamp) {
                (Some(sign), Some(timestamp)) => (sign, timestamp),
                _ => return Err(error),
            },
        };
        let mut url = Url::parse("https://pan.baidu.com/api/sharedownload")
            .map_err(|_| "Baidu download endpoint is invalid.".to_owned())?;
        url.query_pairs_mut()
            .append_pair("sign", &sign)
            .append_pair("timestamp", &timestamp.to_string())
            .append_pair("bdstoken", "")
            .append_pair("channel", "chunlei")
            .append_pair("web", "1")
            .append_pair("app_id", BAIDU_APP_ID)
            .append_pair("clienttype", "0");
        let fid_list = serde_json::to_string(&vec![file.fs_id])
            .map_err(|_| "Unable to encode Baidu file selection.".to_owned())?;
        // randsk arrives percent-encoded; the web client decodes it for sekey.
        let extra = serde_json::to_string(&json!({ "sekey": percent_decode(&self.randsk) }))
            .map_err(|_| "Unable to encode Baidu session proof.".to_owned())?;
        let form = vec![
            ("encrypt".to_owned(), "0".to_owned()),
            ("product".to_owned(), "share".to_owned()),
            ("uk".to_owned(), self.share_uk.to_string()),
            ("primaryid".to_owned(), self.share_id.to_string()),
            ("fid_list".to_owned(), fid_list),
            ("extra".to_owned(), extra),
        ];
        let response = self
            .client
            .post(url)
            .header(USER_AGENT, user_agent)
            .header(REFERER, &self.share.canonical_url)
            .header(COOKIE, &self.cookie_header)
            .form(&form)
            .send()
            .map_err(|error| format!("Unable to resolve Baidu download: {error}"))?;
        if !response.status().is_success() {
            return Err(format!(
                "Baidu download resolver returned HTTP {}.",
                response.status()
            ));
        }
        let resolved: BaiduDownloadResponse = response
            .json()
            .map_err(|_| "Baidu download resolver returned invalid data.".to_owned())?;
        if resolved.errno != 0 {
            return Err(baidu_error("download resolution", resolved.errno));
        }
        let dlink = resolved
            .list
            .into_iter()
            .next()
            .map(|item| item.dlink)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| "Baidu download resolver omitted the download URL.".to_owned())?;
        Ok(DownloadDescriptor {
            provider: CloudDriveProvider::Baidu,
            url: dlink,
            file_name: file.name.clone(),
            headers: vec![
                DownloadHeader {
                    name: "User-Agent".to_owned(),
                    value: user_agent.to_owned(),
                },
                DownloadHeader {
                    name: "Referer".to_owned(),
                    value: self.share.canonical_url.clone(),
                },
                DownloadHeader {
                    name: "Cookie".to_owned(),
                    value: self.cookie_header.clone(),
                },
            ],
            expected_size: Some(file.size),
            expected_sha256: None,
        })
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Cookie names (no values) at each step of opening the share.
    pub fn cookie_trace(&self) -> &str {
        &self.cookie_trace
    }

    fn fetch_download_signature(&self) -> Result<(String, u64), String> {
        let mut url = Url::parse("https://pan.baidu.com/share/tplconfig")
            .map_err(|_| "Baidu signature endpoint is invalid.".to_owned())?;
        url.query_pairs_mut()
            .append_pair("shareid", &self.share_id.to_string())
            .append_pair("uk", &self.share_uk.to_string())
            .append_pair("fields", "sign,timestamp")
            .append_pair("view_mode", "1")
            .append_pair("channel", "chunlei")
            .append_pair("web", "1")
            .append_pair("app_id", BAIDU_APP_ID)
            .append_pair("clienttype", "0");
        let response = self
            .client
            .get(url)
            .header(REFERER, &self.share.canonical_url)
            .header(COOKIE, &self.cookie_header)
            .send()
            .map_err(|error| format!("Unable to request the Baidu download signature: {error}"))?;
        let value: Value = response
            .json()
            .map_err(|_| "Baidu download signature response was invalid.".to_owned())?;
        let errno = value.get("errno").and_then(Value::as_i64).unwrap_or(-1);
        if errno != 0 {
            return Err(baidu_error("download signature", errno));
        }
        let data = value
            .get("data")
            .ok_or_else(|| "Baidu download signature response omitted its data.".to_owned())?;
        let sign = data
            .get("sign")
            .and_then(Value::as_str)
            .filter(|sign| !sign.is_empty() && sign.len() <= 4096)
            .ok_or_else(|| "Baidu download signature response omitted the signature.".to_owned())?;
        let timestamp = data
            .get("timestamp")
            .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok()))
            .ok_or_else(|| "Baidu download signature response omitted the timestamp.".to_owned())?;
        Ok((sign.to_owned(), timestamp))
    }

    fn build_entries(&mut self, items: Vec<BaiduRawItem>) -> Result<Vec<ShareEntry>, String> {
        let mut entries = Vec::with_capacity(items.len());
        for item in items {
            if self.seen_items.len() >= MAX_SHARE_ITEMS {
                return Err("Baidu share exceeds the 10,000-item safety limit.".to_owned());
            }
            let id = format!("baidu:{}", item.fs_id);
            if !self.seen_items.insert(id.clone()) {
                return Err("Baidu share returned duplicate item ids.".to_owned());
            }
            if item.isdir == 1 {
                let children = self.load_directory(&item.path)?;
                entries.push(ShareEntry {
                    id,
                    name: item.server_filename,
                    kind: ShareEntryKind::Folder,
                    size: None,
                    children,
                });
            } else {
                self.files.insert(
                    id.clone(),
                    BaiduFile {
                        fs_id: item.fs_id,
                        name: item.server_filename.clone(),
                        size: item.size,
                    },
                );
                entries.push(ShareEntry {
                    id,
                    name: item.server_filename,
                    kind: ShareEntryKind::File,
                    size: Some(item.size),
                    children: Vec::new(),
                });
            }
        }
        Ok(entries)
    }

    fn load_directory(&mut self, path: &str) -> Result<Vec<ShareEntry>, String> {
        let mut page = 1_u64;
        let mut all = Vec::new();
        loop {
            let mut url = Url::parse("https://pan.baidu.com/share/list")
                .map_err(|_| "Baidu directory endpoint is invalid.".to_owned())?;
            url.query_pairs_mut()
                .append_pair("uk", &self.share_uk.to_string())
                .append_pair("shareid", &self.share_id.to_string())
                .append_pair("order", "other")
                .append_pair("desc", "1")
                .append_pair("showempty", "0")
                .append_pair("web", "1")
                .append_pair("page", &page.to_string())
                .append_pair("num", "100")
                .append_pair("dir", path)
                .append_pair("channel", "chunlei")
                .append_pair("app_id", BAIDU_APP_ID)
                .append_pair("clienttype", "0");
            let response = self
                .client
                .get(url)
                .header(REFERER, &self.share.canonical_url)
                .header(COOKIE, &self.cookie_header)
                .send()
                .map_err(|error| format!("Unable to list Baidu share directory: {error}"))?;
            if !response.status().is_success() {
                return Err(format!(
                    "Baidu share directory returned HTTP {}.",
                    response.status()
                ));
            }
            let listed: BaiduListResponse = response
                .json()
                .map_err(|_| "Baidu share directory returned invalid data.".to_owned())?;
            if listed.errno != 0 {
                return Err(baidu_error("directory listing", listed.errno));
            }
            let count = listed.list.len();
            all.extend(listed.list);
            if self.seen_items.len().saturating_add(all.len()) > MAX_SHARE_ITEMS {
                return Err("Baidu share exceeds the 10,000-item safety limit.".to_owned());
            }
            if !listed.has_more && count < 100 {
                break;
            }
            page += 1;
            if page > 100 {
                return Err("Baidu share directory exceeds the page safety limit.".to_owned());
            }
        }
        self.build_entries(all)
    }
}

fn baidu_surl(share: &ShareLink) -> Result<&str, String> {
    share
        .share_id
        .strip_prefix('1')
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "Baidu share id is invalid.".to_owned())
}

fn validate_authorization_cookie(value: &str) -> Result<String, String> {
    if value.is_empty() || value.len() > 16_384 || value.chars().any(char::is_control) {
        return Err("Baidu authorization cookie is invalid.".to_owned());
    }
    let mut accepted = Vec::new();
    for pair in value.split(';') {
        let Some((name, cookie_value)) = pair.trim().split_once('=') else {
            return Err("Baidu authorization cookie is invalid.".to_owned());
        };
        if !is_authorization_cookie_name(name) || cookie_value.is_empty() {
            return Err("Baidu authorization cookie contains an unsupported field.".to_owned());
        }
        accepted.push(format!("{name}={cookie_value}"));
    }
    if accepted.is_empty() {
        return Err("Baidu authorization cookie is empty.".to_owned());
    }
    Ok(accepted.join("; "))
}

fn baidu_api_url(endpoint: &str, share: &ShareLink) -> Result<Url, String> {
    let mut url = Url::parse(endpoint).map_err(|_| "Baidu endpoint is invalid.".to_owned())?;
    url.query_pairs_mut()
        .append_pair("surl", baidu_surl(share)?)
        .append_pair("channel", "chunlei")
        .append_pair("web", "1")
        .append_pair("app_id", BAIDU_APP_ID)
        .append_pair("bdstoken", "")
        .append_pair("clienttype", "0");
    Ok(url)
}

fn extract_locals_json(html: &str) -> Result<Value, String> {
    let mut merged = serde_json::Map::new();
    for marker in ["locals.mset(", "window.yunData="] {
        let mut offset = 0;
        while let Some(relative_start) = html[offset..].find(marker) {
            let start = offset + relative_start + marker.len();
            let Some(json) = balanced_json_object(&html[start..]) else {
                break;
            };
            if let Ok(Value::Object(object)) = serde_json::from_str::<Value>(json) {
                merged.extend(object);
            }
            offset = start.saturating_add(json.len());
            if offset >= html.len() {
                break;
            }
        }
    }
    if merged.contains_key("shareid") {
        return Ok(Value::Object(merged));
    }
    Err("Verified Baidu share page omitted its structured metadata.".to_owned())
}

fn balanced_json_object(value: &str) -> Option<&str> {
    let start = value.find('{')?;
    let bytes = value.as_bytes();
    let mut depth = 0_u32;
    let mut in_string = false;
    let mut escaped = false;
    for index in start..bytes.len() {
        let byte = bytes[index];
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return value.get(start..=index);
                }
            }
            _ => {}
        }
    }
    None
}

fn json_u64(value: &Value, key: &str) -> Result<u64, String> {
    page_value(value, key)
        .and_then(|entry| {
            entry
                .as_u64()
                .or_else(|| entry.as_str().and_then(|text| text.parse().ok()))
        })
        .ok_or_else(|| format!("Verified Baidu share page omitted {key}."))
}

fn page_value<'a>(value: &'a Value, key: &str) -> Option<&'a Value> {
    value
        .get(key)
        .or_else(|| value.get("data").and_then(|data| data.get(key)))
}

fn page_items(value: &Value) -> Result<Vec<BaiduRawItem>, String> {
    let list = value
        .get("file_list")
        .ok_or_else(|| "Verified Baidu share page omitted its file list.".to_owned())?;
    if let Some(items) = list.as_array() {
        return serde_json::from_value(Value::Array(items.clone()))
            .map_err(|_| "Baidu share file list was invalid.".to_owned());
    }
    if let Some(encoded) = list.as_str() {
        return serde_json::from_str(encoded)
            .map_err(|_| "Baidu share file list was invalid.".to_owned());
    }
    Err("Baidu share file list was invalid.".to_owned())
}

/// `base` with every cookie of `overrides` set, overriding same-named ones.
fn merge_cookie_headers(base: &str, overrides: &str) -> String {
    let parse = |header: &str| -> Vec<(String, String)> {
        header
            .split(';')
            .filter_map(|pair| pair.trim().split_once('='))
            .filter(|(name, _)| !name.trim().is_empty())
            .map(|(name, value)| (name.trim().to_owned(), value.trim().to_owned()))
            .collect()
    };
    let overrides = parse(overrides);
    let mut merged: Vec<(String, String)> = parse(base)
        .into_iter()
        .filter(|(name, _)| !overrides.iter().any(|(other, _)| other == name))
        .collect();
    merged.extend(overrides);
    merged
        .into_iter()
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("; ")
}

/// Decodes %XX escapes (no '+' handling: randsk is not form-encoded).
/// Malformed escapes are kept as written.
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok();
            if let Some(byte) = hex.and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                out.push(byte);
                index += 3;
                continue;
            }
        }
        out.push(bytes[index]);
        index += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| value.to_owned())
}

fn baidu_error(operation: &str, errno: i64) -> String {
    match errno {
        -9 | -10 | -20 | -30 | 113 | 9019 => format!(
            "Baidu {operation} requires controlled account authorization or membership (errno {errno})."
        ),
        -12 | -62 => format!("Baidu {operation} requires a user-completed CAPTCHA (errno {errno})."),
        -64 => format!("Baidu {operation} was blocked by provider risk controls (errno {errno})."),
        _ => format!("Baidu {operation} failed with provider errno {errno}."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sign_in_cookies_survive_a_jar_that_lost_them() {
        assert_eq!(
            merge_cookie_headers("csrfToken=c; BDCLND=r; STOKEN=old", "BDUSS=u; STOKEN=s"),
            "csrfToken=c; BDCLND=r; BDUSS=u; STOKEN=s"
        );
        assert_eq!(merge_cookie_headers("BDCLND=r", ""), "BDCLND=r");
    }

    #[test]
    fn randsk_is_percent_decoded_for_sekey() {
        assert_eq!(percent_decode("ab%2Bcd%2F%3D"), "ab+cd/=");
        assert_eq!(percent_decode("plain"), "plain");
        assert_eq!(percent_decode("bad%zzend%2"), "bad%zzend%2");
    }


    #[test]
    fn extracts_nested_locals_json_without_reading_following_scripts() {
        let html = r#"<script>locals.mset({"shareid":7,"nested":{"text":"a}b"},"file_list":[]});after()</script>"#;
        let value = extract_locals_json(html).unwrap();
        assert_eq!(value["shareid"], 7);
        assert_eq!(value["nested"]["text"], "a}b");
    }

    #[test]
    fn accepts_string_encoded_file_lists() {
        let page = json!({
            "file_list": "[{\"fs_id\":1,\"server_filename\":\"small.txt\",\"path\":\"/small.txt\",\"isdir\":0,\"size\":623}]"
        });
        let items = page_items(&page).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].size, 623);
    }

    #[test]
    fn authorization_cookie_accepts_only_bounded_baidu_fields() {
        let value = validate_authorization_cookie("BDUSS=secret; STOKEN=proof").unwrap();
        assert_eq!(value, "BDUSS=secret; STOKEN=proof");
        assert!(validate_authorization_cookie("OTHER=secret").is_err());
        assert!(validate_authorization_cookie("BDUSS=secret\nInjected=yes").is_err());
    }
}
