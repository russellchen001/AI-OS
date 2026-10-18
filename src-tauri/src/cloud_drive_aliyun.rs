//! Aliyun Drive (阿里云盘 / alipan) share download, using its own web API.
//!
//! Protocol facts were checked against live anonymous calls (2026-10-18) and
//! OpenList's aliyundrive_share driver (AGPL, read as reference only; no code
//! copied). Reading a share needs no sign-in. A signed-in user gets each
//! share file's download URL directly from the share, so nothing is saved to
//! their drive and nothing needs cleaning up.

use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::time::Duration;

const API: &str = "https://api.alipan.com";
pub const ALIYUN_REFERER: &str = "https://www.alipan.com/";
const MAX_ITEMS: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AliyunItem {
    pub file_id: String,
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
}

fn parse_item(value: &Value) -> Option<AliyunItem> {
    let file_id = value.get("file_id").and_then(Value::as_str)?.to_owned();
    let name = value.get("name").and_then(Value::as_str)?.to_owned();
    if file_id.is_empty() || name.is_empty() {
        return None;
    }
    Some(AliyunItem {
        file_id,
        name,
        size: value.get("size").and_then(Value::as_u64).unwrap_or(0),
        is_dir: value.get("type").and_then(Value::as_str) == Some("folder"),
    })
}

/// Aliyun answers errors as `{code: "Name", message}` (often with HTTP 4xx).
fn checked(stage: &str, status: u16, value: Value) -> Result<Value, String> {
    if let Some(code) = value.get("code").and_then(Value::as_str) {
        let message = value.get("message").and_then(Value::as_str).unwrap_or("");
        return Err(format!("阿里云盘{stage}失败（{code}{}）", if message.is_empty() { String::new() } else { format!("，{message}") }));
    }
    if !(200..300).contains(&status) {
        return Err(format!("阿里云盘{stage}失败（HTTP {status}）"));
    }
    Ok(value)
}

pub struct AliyunSession {
    client: Client,
    /// The signed-in web access token; empty for anonymous share reading.
    access_token: String,
}

impl AliyunSession {
    pub fn new(access_token: &str) -> Result<Self, String> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|error| format!("无法创建阿里云盘客户端：{error}"))?;
        Ok(Self { client, access_token: access_token.to_owned() })
    }

    /// One API call; rate limiting (429 / TooManyRequests) is retried with
    /// back-off, as listing a large share hits it.
    fn post(&self, stage: &str, url: &str, share_token: Option<&str>, body: Value) -> Result<Value, String> {
        let mut result = self.post_once(stage, url, share_token, &body);
        for wait in [1, 2, 4, 8] {
            match &result {
                Err(error) if error.contains("TooManyRequests") || error.contains("HTTP 429") => {
                    std::thread::sleep(Duration::from_secs(wait));
                    result = self.post_once(stage, url, share_token, &body);
                }
                _ => break,
            }
        }
        result
    }

    fn post_once(&self, stage: &str, url: &str, share_token: Option<&str>, body: &Value) -> Result<Value, String> {
        let mut request = self.client.post(url).header("Referer", ALIYUN_REFERER).json(body);
        if !self.access_token.is_empty() {
            request = request.header("Authorization", format!("Bearer {}", self.access_token));
        }
        if let Some(token) = share_token {
            request = request.header("x-share-token", token);
        }
        let response = request.send().map_err(|error| format!("阿里云盘请求失败：{error}"))?;
        let status = response.status().as_u16();
        let value = response.json::<Value>().unwrap_or(Value::Null);
        checked(stage, status, value)
    }

    /// True when the access token works (one call for the user's profile).
    pub fn signed_in(&self) -> bool {
        !self.access_token.is_empty()
            && self.post("读取账号", "https://user.alipan.com/v2/user/get", None, json!({})).is_ok()
    }

    /// The share token every share call needs (anonymous).
    pub fn share_token(&self, share_id: &str, share_pwd: &str) -> Result<String, String> {
        let value = self.post(
            "打开分享",
            &format!("{API}/v2/share_link/get_share_token"),
            None,
            json!({ "share_id": share_id, "share_pwd": share_pwd }),
        )?;
        value
            .get("share_token")
            .and_then(Value::as_str)
            .filter(|token| !token.is_empty())
            .map(str::to_owned)
            .ok_or_else(|| "阿里云盘没有返回分享令牌（提取码可能不对）".to_owned())
    }

    /// One folder of a share, all pages (anonymous).
    pub fn share_list(&self, share_id: &str, share_token: &str, parent: &str) -> Result<Vec<AliyunItem>, String> {
        let mut items = Vec::new();
        let mut marker = String::new();
        loop {
            let value = self.post(
                "读取分享文件",
                &format!("{API}/adrive/v2/file/list_by_share"),
                Some(share_token),
                json!({ "share_id": share_id, "parent_file_id": parent, "limit": 100, "marker": marker }),
            )?;
            let list = value.get("items").and_then(Value::as_array).cloned().unwrap_or_default();
            items.extend(list.iter().filter_map(parse_item));
            marker = value.get("next_marker").and_then(Value::as_str).unwrap_or("").to_owned();
            if marker.is_empty() || list.is_empty() {
                return Ok(items);
            }
            if items.len() > MAX_ITEMS {
                return Err("分享超过 10000 个文件".to_owned());
            }
        }
    }

    /// Every file of a share with its folder path, sorted.
    pub fn share_files(&self, share_id: &str, share_token: &str) -> Result<Vec<(String, AliyunItem)>, String> {
        let mut files = Vec::new();
        let mut pending = vec![(String::new(), "root".to_owned())];
        while let Some((dir, parent)) = pending.pop() {
            for item in self.share_list(share_id, share_token, &parent)? {
                if item.is_dir {
                    let path = if dir.is_empty() { item.name.clone() } else { format!("{dir}/{}", item.name) };
                    pending.push((path, item.file_id.clone()));
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

    /// A share file's download URL (needs the signed-in access token).
    pub fn download_url(&self, share_id: &str, share_token: &str, file_id: &str) -> Result<String, String> {
        let value = self.post(
            "获取下载地址",
            &format!("{API}/v2/file/get_share_link_download_url"),
            Some(share_token),
            json!({ "share_id": share_id, "file_id": file_id, "expire_sec": 600 }),
        )?;
        ["download_url", "url"]
            .iter()
            .filter_map(|key| value.get(*key).and_then(Value::as_str))
            .find(|url| url.starts_with("https://"))
            .map(str::to_owned)
            .ok_or_else(|| "阿里云盘没有返回下载地址".to_owned())
    }
}

/// The whole share with the user's signed-in token: every file downloaded
/// into `destination` and verified (size and SHA-256). Same result shape as
/// Baidu and Quark; nothing passes through the user's drive.
pub fn download_share(
    access_token: &str,
    share_id: &str,
    share_pwd: &str,
    destination: &std::path::Path,
    log: impl Fn(&str),
) -> Result<Value, String> {
    use crate::cloud_drive_core::{CloudDriveProvider, DownloadDescriptor, DownloadHeader};
    let session = AliyunSession::new(access_token)?;
    log("open share");
    let share_token = session.share_token(share_id, share_pwd)?;
    let files = session.share_files(share_id, &share_token)?;
    if files.is_empty() {
        return Err("分享里没有可下载的文件".to_owned());
    }
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| format!("无法创建下载客户端：{error}"))?;
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
        let existing = directory.join(&item.name);
        if existing.metadata().is_ok_and(|metadata| metadata.is_file() && metadata.len() == item.size) {
            receipts.push(json!({
                "provider": "aliyun", "path": existing.to_string_lossy(), "bytes": item.size,
                "sha256": crate::cloud_drive::sha256_hex(&existing)?, "attempts": 0, "integrityVerified": true,
            }));
            continue;
        }
        index += 1;
        log(&format!("download {index}/{} {} bytes", todo.len(), item.size));
        // The URL expires in 10 minutes, so it is fetched right before use.
        let url = match session.download_url(share_id, &share_token, &item.file_id) {
            Ok(url) => url,
            Err(error) => {
                skipped.push(skip(error));
                continue;
            }
        };
        let descriptor = DownloadDescriptor {
            provider: CloudDriveProvider::Aliyun,
            url,
            file_name: item.name.clone(),
            headers: vec![DownloadHeader { name: "Referer".to_owned(), value: ALIYUN_REFERER.to_owned() }],
            expected_size: Some(item.size),
            expected_sha256: None,
        };
        match crate::cloud_drive_core::download_file_with_client(&client, &descriptor, &directory, 3, |_| {}) {
            Ok(receipt) => receipts.push(serde_json::to_value(receipt).map_err(|error| error.to_string())?),
            Err(error) => skipped.push(skip(error)),
        }
    }
    Ok(json!({ "receipts": receipts, "skipped": skipped }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_parse_from_aliyun_shapes() {
        let file = parse_item(&json!({"file_id": "f1", "name": "a.exe", "type": "file", "size": 168899651})).unwrap();
        assert_eq!((file.size, file.is_dir), (168899651, false));
        assert!(parse_item(&json!({"file_id": "d", "name": "v3.5.0", "type": "folder"})).unwrap().is_dir);
        assert!(parse_item(&json!({"name": "no id"})).is_none());
    }

    #[test]
    fn aliyun_errors_carry_code_and_status() {
        assert!(checked("x", 200, json!({"items": []})).is_ok());
        assert_eq!(
            checked("打开分享", 404, json!({"code": "NotFound.ShareLink", "message": "gone"})).unwrap_err(),
            "阿里云盘打开分享失败（NotFound.ShareLink，gone）"
        );
        assert_eq!(checked("获取下载地址", 410, Value::Null).unwrap_err(), "阿里云盘获取下载地址失败（HTTP 410）");
    }

    #[test]
    #[ignore = "Requires internet access"]
    fn reads_a_real_public_share_anonymously() {
        // MediaGo's official share, linked from github.com/mediago-dev/mediago docs.
        let session = AliyunSession::new("").unwrap();
        let token = session.share_token("SwCbQBN2nZQ", "").unwrap();
        let files = session.share_files("SwCbQBN2nZQ", &token).unwrap();
        assert!(files.len() > 1);
        assert!(files.iter().all(|(dir, item)| dir.starts_with("MediaGo") && item.size > 0));
        // Downloading needs a signed-in user.
        assert!(session.download_url("SwCbQBN2nZQ", &token, &files[0].1.file_id).is_err());
    }
}
