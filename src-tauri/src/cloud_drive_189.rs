//! 天翼云盘 (cloud.189.cn) share download, using its own web API.
//!
//! Verified with live anonymous calls (2026-10-18, preserve-cd's public share
//! with access code): share info, access-code check and folder listing work
//! without sign-in; the download URL needs the user's signed-in web session
//! (`InvalidSessionKey` otherwise). Files download straight from the share,
//! so nothing passes through the user's drive. 189 lists each file's MD5,
//! which is checked after download.

use reqwest::blocking::{Client, RequestBuilder};
use serde_json::{json, Value};
use std::time::Duration;

const API: &str = "https://cloud.189.cn/api/open";
pub const CLOUD189_REFERER: &str = "https://cloud.189.cn/";
const MAX_ITEMS: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cloud189Item {
    /// 189 ids exceed JavaScript's safe integers; kept as text.
    pub id: String,
    pub name: String,
    pub size: u64,
    pub md5: String,
}

/// What a share code opens: the share id, its root and how to list it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cloud189Share {
    pub share_id: String,
    pub root_id: String,
    pub root_name: String,
    pub is_folder: bool,
    pub share_mode: String,
    pub root_size: u64,
    pub root_md5: String,
}

fn id_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        _ => String::new(),
    }
}

fn checked(stage: &str, value: Value) -> Result<Value, String> {
    if let Some(code) = value.get("errorCode").and_then(Value::as_str) {
        let message = value.get("errorMsg").and_then(Value::as_str).unwrap_or("");
        let hint = if code == "InvalidSessionKey" { "，需要登录天翼云盘" } else { "" };
        return Err(format!("天翼云盘{stage}失败（{code}{hint}{}）", if message.is_empty() || !hint.is_empty() { String::new() } else { format!("，{message}") }));
    }
    let code = match value.get("res_code") {
        Some(Value::Number(number)) => number.as_i64().unwrap_or(-1),
        Some(Value::String(text)) => text.parse().unwrap_or(-1),
        _ => -1,
    };
    if code != 0 {
        let message = value.get("res_message").and_then(Value::as_str).unwrap_or("");
        return Err(format!("天翼云盘{stage}失败（code {code}{}）", if message.is_empty() { String::new() } else { format!("，{message}") }));
    }
    Ok(value)
}

fn parse_file(value: &Value) -> Option<Cloud189Item> {
    let id = id_text(value.get("id"));
    let name = value.get("name").and_then(Value::as_str)?.to_owned();
    if id.is_empty() || name.is_empty() {
        return None;
    }
    Some(Cloud189Item {
        id,
        name,
        size: value.get("size").and_then(Value::as_u64).unwrap_or(0),
        md5: value.get("md5").and_then(Value::as_str).unwrap_or("").to_ascii_lowercase(),
    })
}

pub struct Cloud189Session {
    client: Client,
    /// The signed-in cookie header; empty for anonymous share reading.
    cookie: String,
}

impl Cloud189Session {
    pub fn new(cookie: &str) -> Result<Self, String> {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36")
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|error| format!("无法创建天翼云盘客户端：{error}"))?;
        Ok(Self { client, cookie: cookie.to_owned() })
    }

    fn get(&self, stage: &str, path: &str, query: &[(&str, String)]) -> Result<Value, String> {
        let url = reqwest::Url::parse_with_params(&format!("{API}/{path}"), query)
            .map_err(|error| format!("天翼云盘地址无效：{error}"))?;
        let mut request: RequestBuilder = self
            .client
            .get(url)
            .header("Accept", "application/json;charset=UTF-8")
            .header("Referer", CLOUD189_REFERER);
        if !self.cookie.is_empty() {
            request = request.header("Cookie", &self.cookie);
        }
        let value = request
            .send()
            .and_then(|response| response.json::<Value>())
            .map_err(|error| format!("天翼云盘请求失败：{error}"))?;
        checked(stage, value)
    }

    pub fn signed_in(&self) -> bool {
        !self.cookie.is_empty() && self.get("读取账号", "user/getUserInfoForPortal.action", &[]).is_ok()
    }

    /// Opens a share code (anonymous), checking the access code when needed.
    pub fn open_share(&self, share_code: &str, access_code: &str) -> Result<Cloud189Share, String> {
        let info = self.get("打开分享", "share/getShareInfoByCodeV2.action", &[("shareCode", share_code.to_owned())])?;
        let mut share_id = id_text(info.get("shareId"));
        if share_id.is_empty() {
            if access_code.is_empty() {
                return Err("这个天翼云盘分享需要访问码，请把访问码一起发给我".to_owned());
            }
            let checked_code = self.get(
                "验证访问码",
                "share/checkAccessCode.action",
                &[("shareCode", share_code.to_owned()), ("accessCode", access_code.to_owned())],
            )?;
            share_id = id_text(checked_code.get("shareId"));
        }
        if share_id.is_empty() {
            return Err("天翼云盘没有返回分享编号（访问码可能不对）".to_owned());
        }
        Ok(Cloud189Share {
            share_id,
            root_id: id_text(info.get("fileId")),
            root_name: info.get("fileName").and_then(Value::as_str).unwrap_or("").to_owned(),
            is_folder: info.get("isFolder").and_then(Value::as_bool).unwrap_or(false),
            share_mode: id_text(info.get("shareMode")),
            root_size: info.get("fileSize").and_then(Value::as_u64).unwrap_or(0),
            root_md5: info.get("md5").and_then(Value::as_str).unwrap_or("").to_ascii_lowercase(),
        })
    }

    /// One shared folder: (files, sub-folders as (id, name)), all pages.
    pub fn list_dir(
        &self,
        share: &Cloud189Share,
        access_code: &str,
        dir_id: &str,
    ) -> Result<(Vec<Cloud189Item>, Vec<(String, String)>), String> {
        let mut files = Vec::new();
        let mut folders = Vec::new();
        for page in 1..=200 {
            let value = self.get(
                "读取分享文件",
                "share/listShareDir.action",
                &[
                    ("pageNum", page.to_string()),
                    ("pageSize", "100".to_owned()),
                    ("fileId", dir_id.to_owned()),
                    ("shareDirFileId", dir_id.to_owned()),
                    ("isFolder", "true".to_owned()),
                    ("shareId", share.share_id.clone()),
                    ("shareMode", share.share_mode.clone()),
                    ("iconOption", "5".to_owned()),
                    ("orderBy", "filename".to_owned()),
                    ("descending", "false".to_owned()),
                    ("accessCode", access_code.to_owned()),
                ],
            )?;
            let listing = value.get("fileListAO").cloned().unwrap_or(Value::Null);
            let page_files = listing.get("fileList").and_then(Value::as_array).cloned().unwrap_or_default();
            let page_folders = listing.get("folderList").and_then(Value::as_array).cloned().unwrap_or_default();
            files.extend(page_files.iter().filter_map(parse_file));
            folders.extend(page_folders.iter().filter_map(|folder| {
                let id = id_text(folder.get("id"));
                let name = folder.get("name").and_then(Value::as_str)?.to_owned();
                (!id.is_empty()).then_some((id, name))
            }));
            let count = listing.get("count").and_then(Value::as_u64).unwrap_or(0) as usize;
            if page_files.is_empty() && page_folders.is_empty() || files.len() + folders.len() >= count {
                break;
            }
            if files.len() > MAX_ITEMS {
                return Err("分享超过 10000 个文件".to_owned());
            }
        }
        Ok((files, folders))
    }

    /// Every file of the share with its folder path (the shared folder's own
    /// name first, as on the share page), sorted.
    pub fn share_files(&self, share: &Cloud189Share, access_code: &str) -> Result<Vec<(String, Cloud189Item)>, String> {
        if !share.is_folder {
            return Ok(vec![(
                String::new(),
                Cloud189Item { id: share.root_id.clone(), name: share.root_name.clone(), size: share.root_size, md5: share.root_md5.clone() },
            )]);
        }
        let mut out = Vec::new();
        let mut pending = vec![(share.root_name.clone(), share.root_id.clone())];
        while let Some((dir, id)) = pending.pop() {
            let (files, folders) = self.list_dir(share, access_code, &id)?;
            out.extend(files.into_iter().map(|file| (dir.clone(), file)));
            pending.extend(folders.into_iter().map(|(id, name)| (format!("{dir}/{name}"), id)));
            if out.len() > MAX_ITEMS {
                return Err("分享超过 10000 个文件".to_owned());
            }
        }
        out.sort_by(|a, b| (a.0.as_str(), a.1.name.as_str()).cmp(&(b.0.as_str(), b.1.name.as_str())));
        Ok(out)
    }

    /// A shared file's download URL (needs the signed-in session).
    pub fn download_url(&self, share: &Cloud189Share, file_id: &str) -> Result<String, String> {
        let value = self.get(
            "获取下载地址",
            "file/getFileDownloadUrl.action",
            &[("fileId", file_id.to_owned()), ("dt", "1".to_owned()), ("shareId", share.share_id.clone())],
        )?;
        value
            .get("fileDownloadUrl")
            .and_then(Value::as_str)
            .map(|url| url.replace("&amp;", "&"))
            .map(|url| if url.starts_with("http://") { url.replacen("http://", "https://", 1) } else { url })
            .filter(|url| url.starts_with("https://"))
            .ok_or_else(|| "天翼云盘没有返回下载地址".to_owned())
    }
}

/// The whole share with the user's signed-in cookie: every file downloaded
/// into `destination` and verified by size, SHA-256 and 189's own MD5.
pub fn download_share(
    cookie: &str,
    share_code: &str,
    access_code: &str,
    destination: &std::path::Path,
    log: impl Fn(&str),
) -> Result<Value, String> {
    use crate::cloud_drive_core::{CloudDriveProvider, DownloadDescriptor, DownloadHeader};
    let session = Cloud189Session::new(cookie)?;
    log("open share");
    let share = session.open_share(share_code, access_code)?;
    let files = session.share_files(&share, access_code)?;
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
        let target = directory.join(&item.name);
        let md5_matches = |path: &std::path::Path| item.md5.len() != 32 || crate::cloud_drive_123::md5_hex(path).is_ok_and(|md5| md5 == item.md5);
        if target.metadata().is_ok_and(|metadata| metadata.is_file() && metadata.len() == item.size) && md5_matches(&target) {
            receipts.push(json!({
                "provider": "china-telecom", "path": target.to_string_lossy(), "bytes": item.size,
                "sha256": crate::cloud_drive::sha256_hex(&target)?, "attempts": 0, "integrityVerified": true,
            }));
            continue;
        }
        index += 1;
        log(&format!("download {index}/{} {} bytes", todo.len(), item.size));
        let url = match session.download_url(&share, &item.id) {
            Ok(url) => url,
            Err(error) => {
                skipped.push(skip(error));
                continue;
            }
        };
        let descriptor = DownloadDescriptor {
            provider: CloudDriveProvider::ChinaTelecom,
            url,
            file_name: item.name.clone(),
            headers: vec![DownloadHeader { name: "Referer".to_owned(), value: CLOUD189_REFERER.to_owned() }],
            expected_size: Some(item.size),
            expected_sha256: None,
        };
        match crate::cloud_drive_core::download_file_with_client(&client, &descriptor, &directory, 3, |_| {}) {
            Ok(receipt) if md5_matches(std::path::Path::new(&receipt.path)) => {
                receipts.push(serde_json::to_value(receipt).map_err(|error| error.to_string())?)
            }
            Ok(receipt) => {
                let _ = std::fs::remove_file(&receipt.path);
                skipped.push(skip("下载的文件和天翼云盘记录的 MD5 不一致，已删除".to_owned()));
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
    fn large_ids_stay_exact() {
        let file = parse_file(&json!({"id": 71411112554320027_u64, "name": "a.iso", "size": 680335360, "md5": "66275D47"})).unwrap();
        assert_eq!((file.id.as_str(), file.md5.as_str()), ("71411112554320027", "66275d47"));
    }

    #[test]
    fn errors_name_the_missing_sign_in() {
        let error = checked("获取下载地址", json!({"errorCode": "InvalidSessionKey", "errorMsg": "cookieUserSession is null"})).unwrap_err();
        assert_eq!(error, "天翼云盘获取下载地址失败（InvalidSessionKey，需要登录天翼云盘）");
        assert!(checked("x", json!({"res_code": 0})).is_ok());
        assert!(checked("x", json!({"res_code": "ShareNotFound"})).is_err());
    }

    #[test]
    #[ignore = "Requires internet access"]
    fn reads_a_real_public_share_with_access_code_anonymously() {
        // preserve-cd's share (github.com/skywind3000/preserve-cd README), 口令 jj8t.
        let session = Cloud189Session::new("").unwrap();
        assert!(session.open_share("Ef2IjyuuemIn", "").is_err());
        let share = session.open_share("Ef2IjyuuemIn", "jj8t").unwrap();
        let files = session.share_files(&share, "jj8t").unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].0, "绝版游戏保护工程1");
        assert_eq!((files[0].1.size, files[0].1.md5.as_str()), (680335360, "66275d47779d68614232a3e5d524af4a"));
        assert!(session.download_url(&share, &files[0].1.id).unwrap_err().contains("需要登录"));
    }
}
