//! 123 云盘 share download, using its own web API.
//!
//! Verified with live anonymous calls (2026-10-18, Mixly's public share):
//! the share lists without sign-in, guests get a download link for each
//! file, and the downloaded bytes match the MD5 the share lists as `Etag`.
//! The legacy www.123pan.com/b/api host answers 404; www.123pan.cn serves the
//! same API. When 123 refuses a guest download, the user's own sign-in (kept
//! in AI-OS) is used for the same call.

use base64::Engine;
use reqwest::blocking::{Client, RequestBuilder};
use serde_json::{json, Value};
use std::time::Duration;

const API: &str = "https://www.123pan.cn/b/api";
pub const PAN123_REFERER: &str = "https://www.123pan.cn/";
const MAX_ITEMS: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pan123Item {
    pub file_id: u64,
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    /// The file's MD5 (hex), when 123 lists one.
    pub etag: String,
    pub s3_key_flag: String,
}

fn parse_item(value: &Value) -> Option<Pan123Item> {
    let file_id = value.get("FileId").and_then(Value::as_u64)?;
    let name = value.get("FileName").and_then(Value::as_str)?.to_owned();
    if name.is_empty() {
        return None;
    }
    Some(Pan123Item {
        file_id,
        name,
        size: value.get("Size").and_then(Value::as_u64).unwrap_or(0),
        is_dir: value.get("Type").and_then(Value::as_i64) == Some(1),
        etag: value.get("Etag").and_then(Value::as_str).unwrap_or("").to_ascii_lowercase(),
        s3_key_flag: value.get("S3KeyFlag").and_then(Value::as_str).unwrap_or("").to_owned(),
    })
}

fn checked(stage: &str, value: Value) -> Result<Value, String> {
    let code = value.get("code").and_then(Value::as_i64).unwrap_or(-1);
    // Verified 2026-10-18: 5103 = needs/wrong extraction code, 5104 = share gone.
    if code == 5103 {
        return Err("这个 123 云盘分享需要提取码（或提取码不对），请把链接和提取码一起发给我，例如：`链接 提取码: abcd`".to_owned());
    }
    if code == 5112 {
        return Err("分享者的 123 云盘流量已用完，游客不能下载这个文件；需要登录你自己的 123 云盘账号".to_owned());
    }
    if code == 5104 {
        return Err("这个 123 云盘分享已失效或被取消".to_owned());
    }
    if code != 0 {
        let message = value.get("message").and_then(Value::as_str).unwrap_or("");
        return Err(format!("123 云盘{stage}失败（code {code}{}）", if message.is_empty() { String::new() } else { format!("，{message}") }));
    }
    Ok(value)
}

/// The real file URL inside 123's download page link: `?params=<base64 URL>`.
fn embedded_url(download_url: &str) -> Option<String> {
    let url = reqwest::Url::parse(download_url).ok()?;
    let params = url.query_pairs().find(|(key, _)| key == "params")?.1.into_owned();
    let engine = base64::engine::general_purpose::STANDARD;
    let bytes = engine
        .decode(params.trim_end_matches('='))
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(params.trim_end_matches('=')))
        .or_else(|_| engine.decode(&params))
        .ok()?;
    let inner = String::from_utf8(bytes).ok()?;
    inner.starts_with("https://").then_some(inner)
}

/// 123's CDN sometimes hands out `https://<node>:12349/…`; AI-OS's
/// downloader accepts only default-port HTTPS. The same node serves the same
/// file on the default port (owner test 2026-10-18, verified byte-exact), so
/// the port is dropped.
fn standard_port(url: &str) -> String {
    match reqwest::Url::parse(url) {
        Ok(mut parsed) if parsed.port().is_some() => {
            let _ = parsed.set_port(None);
            parsed.to_string()
        }
        _ => url.to_owned(),
    }
}

pub struct Pan123Session {
    client: Client,
    /// The signed-in web token; empty for guest access.
    token: String,
}

impl Pan123Session {
    pub fn new(token: &str) -> Result<Self, String> {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36")
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|error| format!("无法创建 123 云盘客户端：{error}"))?;
        Ok(Self { client, token: token.trim_matches('"').to_owned() })
    }

    fn send(&self, request: RequestBuilder) -> Result<Value, String> {
        let mut request = request.header("platform", "web").header("App-Version", "3").header("Referer", PAN123_REFERER);
        if !self.token.is_empty() {
            request = request.header("Authorization", format!("Bearer {}", self.token));
        }
        request
            .send()
            .map_err(|error| format!("123 云盘请求失败：{error}"))?
            .json::<Value>()
            .map_err(|error| format!("123 云盘返回了无法读取的结果：{error}"))
    }

    pub fn signed_in(&self) -> bool {
        !self.token.is_empty()
            && self.send(self.client.get(format!("{API}/user/info"))).is_ok_and(|value| checked("读取账号", value).is_ok())
    }

    /// One share folder, all pages (guest access).
    pub fn share_list(&self, share_key: &str, share_pwd: &str, parent: u64) -> Result<Vec<Pan123Item>, String> {
        let mut items = Vec::new();
        for page in 1..=200 {
            let url = reqwest::Url::parse_with_params(
                &format!("{API}/share/get"),
                [
                    ("limit", "100".to_owned()),
                    ("next", "1".to_owned()),
                    ("orderBy", "file_name".to_owned()),
                    ("orderDirection", "asc".to_owned()),
                    ("shareKey", share_key.to_owned()),
                    ("SharePwd", share_pwd.to_owned()),
                    ("ParentFileId", parent.to_string()),
                    ("Page", page.to_string()),
                    ("event", "homeListFile".to_owned()),
                    ("operateType", "1".to_owned()),
                ],
            )
            .map_err(|error| format!("123 云盘地址无效：{error}"))?;
            let value = checked("读取分享文件", self.send(self.client.get(url))?)?;
            let data = value.get("data").cloned().unwrap_or(Value::Null);
            if data.get("Expired").and_then(Value::as_bool) == Some(true) {
                return Err("这个 123 云盘分享已过期".to_owned());
            }
            let list = data.get("InfoList").and_then(Value::as_array).cloned().unwrap_or_default();
            items.extend(list.iter().filter_map(parse_item));
            let next = data.get("Next").and_then(Value::as_str).unwrap_or("-1");
            if list.is_empty() || next == "-1" {
                return Ok(items);
            }
            if items.len() > MAX_ITEMS {
                return Err("分享超过 10000 个文件".to_owned());
            }
        }
        Ok(items)
    }

    /// Every file of a share with its folder path, sorted.
    pub fn share_files(&self, share_key: &str, share_pwd: &str) -> Result<Vec<(String, Pan123Item)>, String> {
        let mut files = Vec::new();
        let mut pending = vec![(String::new(), 0_u64)];
        while let Some((dir, parent)) = pending.pop() {
            for item in self.share_list(share_key, share_pwd, parent)? {
                if item.is_dir {
                    let path = if dir.is_empty() { item.name.clone() } else { format!("{dir}/{}", item.name) };
                    pending.push((path, item.file_id));
                } else {
                    files.push((dir.clone(), item));
                }
                if files.len() > MAX_ITEMS {
                    return Err("分享超过 10000 个文件".to_owned());
                }
            }
        }
        files.sort_by(|a, b| (a.0.as_str(), a.1.name.as_str()).cmp(&(b.0.as_str(), b.1.name.as_str())));
        Ok(files)
    }

    /// The file URL to download: 123's download link, then its embedded
    /// address, which answers with the final `redirect_url`.
    pub fn file_url(&self, share_key: &str, item: &Pan123Item) -> Result<String, String> {
        let value = checked(
            "获取下载地址",
            self.send(self.client.post(format!("{API}/share/download/info")).json(&json!({
                "ShareKey": share_key,
                "FileID": item.file_id,
                "S3keyFlag": item.s3_key_flag,
                "Size": item.size,
                "Etag": item.etag,
            })))?,
        )?;
        let link = value
            .pointer("/data/DownloadURL")
            .and_then(Value::as_str)
            .filter(|url| url.starts_with("https://"))
            .ok_or_else(|| "123 云盘没有返回下载地址".to_owned())?;
        let Some(inner) = embedded_url(link) else { return Ok(link.to_owned()) };
        let answer = self
            .client
            .get(&inner)
            .header("Referer", PAN123_REFERER)
            .send()
            .and_then(|response| response.json::<Value>())
            .map_err(|error| format!("123 云盘下载地址无法解析：{error}"))?;
        answer
            .pointer("/data/redirect_url")
            .and_then(Value::as_str)
            .filter(|url| url.starts_with("https://"))
            .map(standard_port)
            .ok_or_else(|| "123 云盘没有返回最终下载地址".to_owned())
    }
}

pub(crate) fn md5_hex(path: &std::path::Path) -> Result<String, String> {
    use md5::{Digest, Md5};
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|error| format!("无法校验下载的文件：{error}"))?;
    let mut hash = Md5::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|error| format!("无法校验下载的文件：{error}"))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hash.finalize().iter().map(|byte| format!("{byte:02x}")).collect())
}

/// The whole share: guest download first; files 123 refuses to guests are
/// retried with `signed_in_token()` (asked for at most once). Each file is
/// verified by size, SHA-256 and, when listed, the share's own MD5.
pub fn download_share(
    share_key: &str,
    share_pwd: &str,
    destination: &std::path::Path,
    mut signed_in_token: impl FnMut() -> Result<String, String>,
    log: impl Fn(&str),
) -> Result<Value, String> {
    use crate::cloud_drive_core::{CloudDriveProvider, DownloadDescriptor, DownloadHeader};
    let guest = Pan123Session::new("")?;
    log("open share");
    let files = guest.share_files(share_key, share_pwd)?;
    if files.is_empty() {
        return Err("分享里没有可下载的文件".to_owned());
    }
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| format!("无法创建下载客户端：{error}"))?;
    let mut member: Option<Result<Pan123Session, String>> = None;
    let mut receipts = Vec::new();
    let mut skipped = Vec::new();
    // Progress: how many files (and bytes) still have to arrive.
    let todo: Vec<u64> = files
        .iter()
        .filter(|(dir, item)| !crate::cloud_drive::already_downloaded(destination, dir, &item.name, item.size))
        .map(|(_, item)| item.size)
        .collect();
    log(&format!("plan {} files {} bytes", todo.len(), todo.iter().sum::<u64>()));
    let mut index = 0;
    for (dir, item) in &files {
        let shown = if dir.is_empty() { item.name.clone() } else { format!("{dir}/{}", item.name) };
        let skip = |reason: String| json!({ "name": shown, "bytes": item.size, "reason": reason });
        let Some(directory) = crate::cloud_drive::safe_relative_dir(dir) else {
            skipped.push(skip("文件夹名称不安全".to_owned()));
            continue;
        };
        let directory = destination.join(directory);
        let target = directory.join(&item.name);
        let md5_matches = |path: &std::path::Path| item.etag.len() != 32 || md5_hex(path).is_ok_and(|md5| md5 == item.etag);
        if target.metadata().is_ok_and(|metadata| metadata.is_file() && metadata.len() == item.size) && md5_matches(&target) {
            receipts.push(json!({
                "provider": "pan123", "path": target.to_string_lossy(), "bytes": item.size,
                "sha256": crate::cloud_drive::sha256_hex(&target)?, "attempts": 0, "integrityVerified": true,
            }));
            continue;
        }
        index += 1;
        log(&format!("download {index}/{} {} bytes", todo.len(), item.size));
        let url = match guest.file_url(share_key, item) {
            Ok(url) => Ok(url),
            Err(guest_error) => {
                let session = member.get_or_insert_with(|| {
                    log("guest refused; using sign-in");
                    signed_in_token().and_then(|token| Pan123Session::new(&token))
                });
                match session {
                    Ok(session) => session.file_url(share_key, item),
                    Err(error) => Err(format!("{guest_error}；{error}")),
                }
            }
        };
        let url = match url {
            Ok(url) => url,
            Err(error) => {
                skipped.push(skip(error));
                continue;
            }
        };
        let descriptor = DownloadDescriptor {
            provider: CloudDriveProvider::Pan123,
            url,
            file_name: item.name.clone(),
            headers: vec![DownloadHeader { name: "Referer".to_owned(), value: PAN123_REFERER.to_owned() }],
            expected_size: Some(item.size),
            expected_sha256: None,
        };
        match crate::cloud_drive_core::download_file_with_client(&client, &descriptor, &directory, 3, |_| {}) {
            Ok(receipt) if md5_matches(std::path::Path::new(&receipt.path)) => {
                receipts.push(serde_json::to_value(receipt).map_err(|error| error.to_string())?)
            }
            Ok(receipt) => {
                let _ = std::fs::remove_file(&receipt.path);
                skipped.push(skip("下载的文件和 123 云盘记录的 MD5 不一致，已删除".to_owned()));
            }
            Err(error) => skipped.push(skip(error)),
        }
    }
    Ok(json!({ "receipts": receipts, "skipped": skipped }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_parse_from_123_shapes() {
        let item = parse_item(&json!({"FileId": 7150981, "FileName": "a.zip", "Size": 116498, "Type": 0, "Etag": "ED38", "S3KeyFlag": "1-0"})).unwrap();
        assert_eq!((item.file_id, item.size, item.is_dir, item.etag.as_str()), (7150981, 116498, false, "ed38"));
        assert!(parse_item(&json!({"FileId": 1, "FileName": "d", "Type": 1})).unwrap().is_dir);
        assert!(parse_item(&json!({"FileName": "no id"})).is_none());
    }

    #[test]
    fn embedded_download_address_is_decoded() {
        let inner = "https://guest.example.com/file?v=5&t=1";
        let link = format!(
            "https://web-pro2.example.com/download-v2/?params={}&is_s3=0",
            base64::engine::general_purpose::STANDARD.encode(inner)
        );
        assert_eq!(embedded_url(&link).as_deref(), Some(inner));
        assert_eq!(embedded_url("https://cdn.example.com/plain.zip"), None);
    }

    #[test]
    fn html_suffixed_share_links_parse() {
        let share = crate::cloud_drive_core::ShareLink::parse("https://www.123pan.com/s/Pf5Yjv-Bb9W3.html", None).unwrap();
        assert_eq!(share.share_id, "Pf5Yjv-Bb9W3");
    }

    #[test]
    fn extraction_code_and_gone_shares_are_explained() {
        assert!(checked("读取分享文件", json!({"code": 5103, "message": "提取码错误"})).unwrap_err().contains("需要提取码"));
        assert!(checked("读取分享文件", json!({"code": 5104})).unwrap_err().contains("已失效"));
    }

    #[test]
    fn cdn_links_use_the_default_https_port() {
        assert_eq!(
            standard_port("https://101-254-181-179-v3.pd1.cjjd19.com:12349/guest/a.zip?v=5&t=1"),
            "https://101-254-181-179-v3.pd1.cjjd19.com/guest/a.zip?v=5&t=1"
        );
        assert_eq!(standard_port("https://cdn.example.com/a.zip"), "https://cdn.example.com/a.zip");
    }

    #[test]
    fn errors_carry_code_and_message() {
        assert_eq!(
            checked("获取下载地址", json!({"code": 401, "message": "请登录"})).unwrap_err(),
            "123 云盘获取下载地址失败（code 401，请登录）"
        );
    }

    #[test]
    #[ignore = "Requires internet access; downloads about 166 MB"]
    fn downloads_a_whole_real_share_as_guest() {
        // Owner test share 2026-10-18 (12 files, some on CDN port 12349).
        let destination = std::env::temp_dir().join(format!("ai-os-123-share-{}", std::process::id()));
        let result = download_share("3CSxjv-cMpVv", "", &destination, || Err("no sign-in in tests".to_owned()), |_| {}).unwrap();
        let _ = std::fs::remove_dir_all(&destination);
        assert_eq!(result["skipped"].as_array().unwrap().len(), 0, "{result}");
        assert_eq!(result["receipts"].as_array().unwrap().len(), 12);
    }

    #[test]
    #[ignore = "Requires internet access"]
    fn downloads_a_real_public_file_as_guest_and_matches_its_md5() {
        // Mixly's official share (github.com/mixly/Mixly_Arduino README).
        let destination = std::env::temp_dir().join(format!("ai-os-123-test-{}", std::process::id()));
        let session = Pan123Session::new("").unwrap();
        let folder = session.share_list("uAxvTd-8229d", "", 7151713).unwrap();
        let file = folder.iter().find(|item| item.file_id == 7150981).cloned().unwrap();
        let url = session.file_url("uAxvTd-8229d", &file).unwrap();
        let descriptor = crate::cloud_drive_core::DownloadDescriptor {
            provider: crate::cloud_drive_core::CloudDriveProvider::Pan123,
            url,
            file_name: file.name.clone(),
            headers: vec![],
            expected_size: Some(file.size),
            expected_sha256: None,
        };
        let client = Client::new();
        let receipt = crate::cloud_drive_core::download_file_with_client(&client, &descriptor, &destination, 2, |_| {}).unwrap();
        assert_eq!(md5_hex(std::path::Path::new(&receipt.path)).unwrap(), file.etag);
        let _ = std::fs::remove_dir_all(destination);
    }
}
