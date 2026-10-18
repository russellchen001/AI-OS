//! 移动云盘 (中国移动云盘 / 和彩云) share download.
//!
//! Verified with live calls (2026-10-18, SwashbucklerDiary's public share):
//! the share service takes and returns AES-128-CBC encrypted JSON with a
//! fixed public key (random IV prefixed, base64, sent as a JSON string).
//! Listing works anonymously; the download link needs the user's signed-in
//! web credential (the `authorization` cookie of yun.139.com), otherwise the
//! service answers 200000401. Protocol facts were checked against pansou
//! (MIT) and OpenList (AGPL, reference only); no code was copied.

use aes::Aes128;
use base64::Engine;
use cbc::cipher::{block_padding::Pkcs7, BlockModeDecrypt, BlockModeEncrypt, KeyIvInit};
use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::time::Duration;

const SHARE_API: &str = "https://share-kd-njs.yun.139.com/yun-share/richlifeApp/devapp/IOutLink";
const SHARE_KEY: &[u8; 16] = b"PVGDwmcvfs1uV3d1";
pub const CAIYUN_REFERER: &str = "https://yun.139.com/";
const PAGE: u64 = 200;
const MAX_ITEMS: usize = 10_000;

fn encrypt(plain: &str) -> String {
    let iv: [u8; 16] = rand_iv();
    let cipher = cbc::Encryptor::<Aes128>::new(SHARE_KEY.into(), &iv.into());
    let mut out = iv.to_vec();
    out.extend(cipher.encrypt_padded_vec::<Pkcs7>(plain.as_bytes()));
    base64::engine::general_purpose::STANDARD.encode(out)
}

fn decrypt(text: &str) -> Result<String, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(text.trim().trim_matches('"'))
        .map_err(|_| "移动云盘返回了无法解码的结果".to_owned())?;
    if bytes.len() < 32 || bytes.len() % 16 != 0 {
        return Err("移动云盘返回的结果长度异常".to_owned());
    }
    let (iv, body) = bytes.split_at(16);
    let iv: [u8; 16] = iv.try_into().map_err(|_| "移动云盘返回的结果长度异常".to_owned())?;
    let cipher = cbc::Decryptor::<Aes128>::new(SHARE_KEY.into(), &iv.into());
    let plain = cipher
        .decrypt_padded_vec::<Pkcs7>(body)
        .map_err(|_| "移动云盘返回的结果无法解密".to_owned())?;
    String::from_utf8(plain).map_err(|_| "移动云盘返回的结果不是文本".to_owned())
}

fn rand_iv() -> [u8; 16] {
    let mut iv = [0_u8; 16];
    let seed = uuid::Uuid::new_v4();
    iv.copy_from_slice(seed.as_bytes());
    iv
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaiyunItem {
    /// The content path the download call takes ("<folder>/<file>").
    pub path: String,
    pub name: String,
    pub size: u64,
}

/// The signed-in credential: the web `authorization` cookie, whose base64
/// text is "<client>:<phone>:<token>". Returns (header value, account).
pub fn credential_from_cookies(cookie_header: &str) -> Option<(String, String)> {
    let value = cookie_header
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| name.trim() == "authorization")?
        .1
        .trim()
        .trim_matches('"');
    let token = value.strip_prefix("Basic ").or_else(|| value.strip_prefix("Basic%20")).unwrap_or(value);
    let decoded = base64::engine::general_purpose::STANDARD.decode(token).ok()?;
    let account = String::from_utf8(decoded).ok()?.split(':').nth(1)?.to_owned();
    (!account.is_empty()).then(|| (format!("Basic {token}"), account))
}

pub struct CaiyunSession {
    client: Client,
    /// (Authorization header, account) once signed in.
    credential: Option<(String, String)>,
}

impl CaiyunSession {
    pub fn new(credential: Option<(String, String)>) -> Result<Self, String> {
        let client = Client::builder()
            .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|error| format!("无法创建移动云盘客户端：{error}"))?;
        Ok(Self { client, credential })
    }

    fn account(&self) -> String {
        self.credential.as_ref().map(|(_, account)| account.clone()).unwrap_or_default()
    }

    fn call(&self, stage: &str, endpoint: &str, request: Value) -> Result<Value, String> {
        let mut builder = self
            .client
            .post(format!("{SHARE_API}/{endpoint}"))
            .header("Accept", "application/json, text/plain, */*")
            .header("Content-Type", "application/json;charset=UTF-8")
            .header("hcy-cool-flag", "1")
            .header("x-deviceinfo", "||9|12.27.0|chrome|131.0.0.0|||mac os|1920X1080|zh-CN|||")
            .header("Origin", "https://yun.139.com")
            .header("Referer", CAIYUN_REFERER)
            .body(Value::String(encrypt(&request.to_string())).to_string());
        if let Some((authorization, _)) = &self.credential {
            builder = builder.header("Authorization", authorization);
        }
        let text = builder
            .send()
            .and_then(|response| response.text())
            .map_err(|error| format!("移动云盘请求失败：{error}"))?;
        let value: Value = serde_json::from_str(&decrypt(&text)?).map_err(|_| "移动云盘返回了无法读取的结果".to_owned())?;
        let code = value.get("resultCode").and_then(Value::as_str).unwrap_or("");
        if code != "0" {
            let desc = value.get("desc").and_then(Value::as_str).unwrap_or("");
            let hint = if code == "200000401" { "，需要登录移动云盘" } else { "" };
            return Err(format!("移动云盘{stage}失败（{code}{hint}{}）", if desc.is_empty() || !hint.is_empty() { String::new() } else { format!("，{desc}") }));
        }
        Ok(value.get("data").cloned().unwrap_or(Value::Null))
    }

    /// One folder of the share ("root" for the top): (files, folders as
    /// (id, name)), all pages.
    pub fn list(&self, link_id: &str, passwd: &str, folder: &str) -> Result<(Vec<CaiyunItem>, Vec<(String, String)>), String> {
        let mut files = Vec::new();
        let mut folders = Vec::new();
        let mut begin = 1;
        loop {
            let data = self.call(
                "读取分享文件",
                "getOutLinkInfoV6",
                json!({
                    "getOutLinkInfoReq": {
                        "account": self.account(), "linkID": link_id, "passwd": passwd,
                        "caSrt": 1, "coSrt": 1, "srtDr": 0, "bNum": begin, "pCaID": folder, "eNum": begin + PAGE - 1,
                    },
                    "commonAccountInfo": { "account": self.account(), "accountType": 1 },
                }),
            )?;
            let page_folders = data.get("caLst").and_then(Value::as_array).cloned().unwrap_or_default();
            let page_files = data.get("coLst").and_then(Value::as_array).cloned().unwrap_or_default();
            folders.extend(page_folders.iter().filter_map(|folder| {
                Some((folder.get("caID")?.as_str()?.to_owned(), folder.get("caName")?.as_str()?.to_owned()))
            }));
            files.extend(page_files.iter().filter_map(|file| {
                Some(CaiyunItem {
                    path: file.get("path")?.as_str()?.to_owned(),
                    name: file.get("coName")?.as_str()?.to_owned(),
                    size: file.get("coSize").and_then(Value::as_u64).unwrap_or(0),
                })
            }));
            if (page_files.len() + page_folders.len()) < PAGE as usize {
                return Ok((files, folders));
            }
            if files.len() > MAX_ITEMS {
                return Err("分享超过 10000 个文件".to_owned());
            }
            begin += PAGE;
        }
    }

    /// Every file of the share with its folder path, sorted.
    pub fn share_files(&self, link_id: &str, passwd: &str) -> Result<Vec<(String, CaiyunItem)>, String> {
        let mut out = Vec::new();
        let mut pending = vec![(String::new(), "root".to_owned())];
        while let Some((dir, folder)) = pending.pop() {
            let (files, folders) = self.list(link_id, passwd, &folder)?;
            out.extend(files.into_iter().map(|file| (dir.clone(), file)));
            pending.extend(folders.into_iter().map(|(id, name)| {
                (if dir.is_empty() { name } else { format!("{dir}/{name}") }, id)
            }));
            if out.len() > MAX_ITEMS {
                return Err("分享超过 10000 个文件".to_owned());
            }
        }
        out.sort_by(|a, b| (a.0.as_str(), a.1.name.as_str()).cmp(&(b.0.as_str(), b.1.name.as_str())));
        Ok(out)
    }

    /// A shared file's download URL (needs the signed-in credential).
    pub fn download_url(&self, link_id: &str, passwd: &str, path: &str) -> Result<String, String> {
        let data = self.call(
            "获取下载地址",
            "dlFromOutLinkV3",
            json!({
                "dlFromOutLinkReqV3": { "account": self.account(), "linkID": link_id, "passwd": passwd, "coIDLst": { "item": [path] } },
                "commonAccountInfo": { "account": self.account(), "accountType": 1 },
            }),
        )?;
        ["/extInfo/cdnDownloadUrl", "/redrUrl", "/downloadUrl"]
            .iter()
            .filter_map(|pointer| data.pointer(pointer).and_then(Value::as_str))
            .map(|url| if url.starts_with("http://") { url.replacen("http://", "https://", 1) } else { url.to_owned() })
            .find(|url| url.starts_with("https://"))
            .ok_or_else(|| "移动云盘没有返回下载地址".to_owned())
    }
}

/// The whole share with the user's credential: every file downloaded into
/// `destination` and verified (size and SHA-256).
pub fn download_share(
    credential: (String, String),
    link_id: &str,
    passwd: &str,
    destination: &std::path::Path,
    log: impl Fn(&str),
) -> Result<Value, String> {
    use crate::cloud_drive_core::{CloudDriveProvider, DownloadDescriptor, DownloadHeader};
    let session = CaiyunSession::new(Some(credential))?;
    log("open share");
    let files = session.share_files(link_id, passwd)?;
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
        if target.metadata().is_ok_and(|metadata| metadata.is_file() && metadata.len() == item.size) {
            receipts.push(json!({
                "provider": "china-mobile", "path": target.to_string_lossy(), "bytes": item.size,
                "sha256": crate::cloud_drive::sha256_hex(&target)?, "attempts": 0, "integrityVerified": true,
            }));
            continue;
        }
        index += 1;
        log(&format!("download {index}/{} {} bytes", todo.len(), item.size));
        let url = match session.download_url(link_id, passwd, &item.path) {
            Ok(url) => url,
            Err(error) => {
                skipped.push(skip(error));
                continue;
            }
        };
        let descriptor = DownloadDescriptor {
            provider: CloudDriveProvider::ChinaMobile,
            url,
            file_name: item.name.clone(),
            headers: vec![DownloadHeader { name: "Referer".to_owned(), value: CAIYUN_REFERER.to_owned() }],
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
    fn payloads_round_trip_through_the_share_cipher() {
        let text = r#"{"linkID":"2u8omYzvLhqdo","名":"侠客日记"}"#;
        let sealed = encrypt(text);
        assert_ne!(encrypt(text), sealed, "a fresh IV each time");
        assert_eq!(decrypt(&sealed).unwrap(), text);
        assert_eq!(decrypt(&format!("\"{sealed}\"")).unwrap(), text);
        assert!(decrypt("bm90IGVuY3J5cHRlZA==").is_err());
    }

    #[test]
    fn credential_comes_from_the_authorization_cookie() {
        let token = base64::engine::general_purpose::STANDARD.encode("pc:13800000000:secret|1|RCS|1");
        let (header, account) = credential_from_cookies(&format!("a=1; authorization=Basic {token}; b=2")).unwrap();
        assert_eq!((header, account.as_str()), (format!("Basic {token}"), "13800000000"));
        assert!(credential_from_cookies("a=1").is_none());
    }

    #[test]
    fn all_share_link_forms_parse() {
        for (link, id) in [
            ("https://yun.139.com/shareweb/#/w/i/2u8omYzvLhqdo", "2u8omYzvLhqdo"),
            ("https://caiyun.139.com/m/i?015CHx2VU4RNd", "015CHx2VU4RNd"),
            ("https://caiyun.139.com/w/i/2qidZUhfLSS6i", "2qidZUhfLSS6i"),
        ] {
            assert_eq!(crate::cloud_drive_core::ShareLink::parse(link, None).unwrap().share_id, id, "{link}");
        }
    }

    #[test]
    #[ignore = "Requires internet access"]
    fn reads_a_real_public_share_anonymously() {
        // SwashbucklerDiary's share (github.com/Yu-Core/SwashbucklerDiary README), 提取码 3dgn.
        let session = CaiyunSession::new(None).unwrap();
        let files = session.share_files("2u8omYzvLhqdo", "3dgn").unwrap();
        assert!(files.len() > 3);
        assert!(files.iter().all(|(dir, item)| dir == "侠客日记" && item.size > 0 && item.path.contains('/')));
        assert!(session.download_url("2u8omYzvLhqdo", "3dgn", &files[0].1.path).unwrap_err().contains("需要登录"));
    }
}
