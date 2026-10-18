//! Quark Netdisk and UC Netdisk share download, using their own web API
//! (UC's drive runs on the same service as Quark's, with its own host and
//! product code).
//!
//! Protocol facts (endpoints, parameter names, client identity) were checked
//! against live anonymous calls and Quark's web client; no third-party code is
//! copied. Reading a share needs no sign-in. Downloading needs the user's own
//! signed-in session: files are saved into a temporary folder in the user's
//! drive, downloaded, verified and the folder deleted, as with Baidu.

use reqwest::blocking::Client;
use serde_json::{json, Value};
use std::time::{Duration, Instant};

/// One drive on the Quark service.
pub struct Flavor {
    /// Shown in messages.
    pub name: &'static str,
    /// The provider key used in download receipts.
    pub key: &'static str,
    pub provider: crate::cloud_drive_core::CloudDriveProvider,
    api: &'static str,
    /// Product code: a share is only found under its own drive's code.
    pr: &'static str,
    referer: &'static str,
    /// The drive's desktop client identity; the web API expects it.
    user_agent: &'static str,
}

pub const QUARK: Flavor = Flavor {
    name: "夸克",
    key: "quark",
    provider: crate::cloud_drive_core::CloudDriveProvider::Quark,
    api: "https://drive-pc.quark.cn/1/clouddrive",
    pr: "ucpro",
    referer: "https://pan.quark.cn/",
    user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) quark-cloud-drive/3.14.2 Chrome/112.0.5615.165 Electron/24.1.3.8 Safari/537.36 Channel/pckk_other_ch",
};

/// Verified 2026-10-18: UC shares open only with pr=UCBrowser on pc-api.uc.cn.
pub const UC: Flavor = Flavor {
    name: "UC 网盘",
    key: "uc",
    provider: crate::cloud_drive_core::CloudDriveProvider::Uc,
    api: "https://pc-api.uc.cn/1/clouddrive",
    pr: "UCBrowser",
    referer: "https://drive.uc.cn/",
    user_agent: "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36",
};
const MAX_ITEMS: usize = 10_000;

/// One file or folder in a share or in the user's drive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuarkItem {
    pub fid: String,
    pub share_fid_token: String,
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
}

pub struct QuarkSession {
    flavor: &'static Flavor,
    client: Client,
    /// The signed-in cookie header, kept current with the cookies Quark sets
    /// on each response; empty for anonymous share reading.
    cookie: std::sync::Mutex<String>,
}

fn field_str(value: &Value, key: &str) -> String {
    match value.get(key) {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        _ => String::new(),
    }
}

fn parse_item(value: &Value) -> Option<QuarkItem> {
    let fid = field_str(value, "fid");
    let name = field_str(value, "file_name");
    if fid.is_empty() || name.is_empty() {
        return None;
    }
    Some(QuarkItem {
        fid,
        share_fid_token: field_str(value, "share_fid_token"),
        name,
        size: value.get("size").and_then(Value::as_u64).unwrap_or(0),
        is_dir: value.get("dir").and_then(Value::as_bool).unwrap_or(false)
            || value.get("file_type").and_then(Value::as_i64) == Some(0),
    })
}

/// Quark answers `{code: 0, data: …}`; anything else is reported with its message.
fn checked(name: &str, stage: &str, value: Value) -> Result<Value, String> {
    let code = value.get("code").and_then(Value::as_i64).unwrap_or(-1);
    if code != 0 {
        let message = value.get("message").and_then(Value::as_str).unwrap_or("");
        return Err(format!("{name}{stage}失败（code {code}{}）", if message.is_empty() { String::new() } else { format!("，{message}") }));
    }
    Ok(value)
}

impl QuarkSession {
    pub fn new(flavor: &'static Flavor, cookie: &str) -> Result<Self, String> {
        let client = Client::builder()
            .user_agent(flavor.user_agent)
            .connect_timeout(Duration::from_secs(20))
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|error| format!("无法创建{}客户端：{error}", flavor.name))?;
        Ok(Self { flavor, client, cookie: std::sync::Mutex::new(cookie.to_owned()) })
    }

    pub fn cookie(&self) -> String {
        self.cookie.lock().map(|cookie| cookie.clone()).unwrap_or_default()
    }

    fn send(&self, request: reqwest::blocking::RequestBuilder) -> Result<Value, String> {
        let cookie = self.cookie();
        let request = if cookie.is_empty() { request } else { request.header("Cookie", cookie) };
        let response = request.send().map_err(|error| format!("{}请求失败：{error}", self.flavor.name))?;
        let set_cookies = response
            .headers()
            .get_all("set-cookie")
            .iter()
            .filter_map(|value| value.to_str().ok())
            .filter_map(|value| value.split(';').next())
            .collect::<Vec<_>>()
            .join("; ");
        if !set_cookies.is_empty() {
            if let Ok(mut cookie) = self.cookie.lock() {
                if !cookie.is_empty() {
                    *cookie = crate::cloud_drive_baidu::merge_cookie_headers(&cookie, &set_cookies);
                }
            }
        }
        response.json::<Value>().map_err(|error| format!("{}返回了无法读取的结果：{error}", self.flavor.name))
    }

    fn get(&self, path: &str, query: &[(&str, String)]) -> Result<Value, String> {
        let pairs = self.base_query().into_iter().chain(query.iter().cloned());
        let url = reqwest::Url::parse_with_params(&format!("{}/{path}", self.flavor.api), pairs)
            .map_err(|error| format!("网盘地址无效：{error}"))?;
        self.send(self.client.get(url).header("Referer", self.flavor.referer))
    }

    fn post(&self, path: &str, body: Value) -> Result<Value, String> {
        let url = reqwest::Url::parse_with_params(&format!("{}/{path}", self.flavor.api), self.base_query())
            .map_err(|error| format!("网盘地址无效：{error}"))?;
        self.send(self.client.post(url).header("Referer", self.flavor.referer).json(&body))
    }

    fn base_query(&self) -> Vec<(&'static str, String)> {
        vec![("pr", self.flavor.pr.to_owned()), ("fr", "pc".to_owned()), ("uc_param_str", String::new())]
    }

    /// Opens a share (anonymous): the share token used by every share call.
    pub fn share_token(&self, pwd_id: &str, passcode: &str) -> Result<String, String> {
        let value = self.post("share/sharepage/token", json!({ "pwd_id": pwd_id, "passcode": passcode }))?;
        let value = checked(self.flavor.name, "打开分享", value)?;
        let stoken = value.pointer("/data/stoken").and_then(Value::as_str).unwrap_or("");
        if stoken.is_empty() {
            return Err(format!("{}没有返回分享令牌（提取码可能不对）", self.flavor.name));
        }
        Ok(stoken.to_owned())
    }

    /// Items of one share folder (anonymous), all pages.
    pub fn share_list(&self, pwd_id: &str, stoken: &str, pdir_fid: &str) -> Result<Vec<QuarkItem>, String> {
        let mut items = Vec::new();
        for page in 1..=200 {
            let value = checked(self.flavor.name, 
                "读取分享文件",
                self.get(
                    "share/sharepage/detail",
                    &[
                        ("pwd_id", pwd_id.to_owned()),
                        ("stoken", stoken.to_owned()),
                        ("pdir_fid", pdir_fid.to_owned()),
                        ("force", "0".to_owned()),
                        ("_page", page.to_string()),
                        ("_size", "50".to_owned()),
                        ("_fetch_total", "1".to_owned()),
                        ("_sort", "file_type:asc,file_name:asc".to_owned()),
                    ],
                )?,
            )?;
            let list = value.pointer("/data/list").and_then(Value::as_array).cloned().unwrap_or_default();
            let total = value.pointer("/metadata/_total").and_then(Value::as_u64).unwrap_or(0) as usize;
            items.extend(list.iter().filter_map(parse_item));
            if list.is_empty() || items.len() >= total || items.len() > MAX_ITEMS {
                break;
            }
        }
        Ok(items)
    }

    /// Every file of a share with its folder path, plus the share's top items
    /// (what gets saved).
    pub fn share_files(
        &self,
        pwd_id: &str,
        stoken: &str,
    ) -> Result<(Vec<QuarkItem>, Vec<(String, QuarkItem)>), String> {
        let top = self.share_list(pwd_id, stoken, "0")?;
        let mut files = Vec::new();
        let mut pending: Vec<(String, QuarkItem)> = top.iter().map(|item| (String::new(), item.clone())).collect();
        while let Some((dir, item)) = pending.pop() {
            if files.len() > MAX_ITEMS {
                return Err("分享超过 10000 个文件".to_owned());
            }
            if item.is_dir {
                let path = if dir.is_empty() { item.name.clone() } else { format!("{dir}/{}", item.name) };
                for child in self.share_list(pwd_id, stoken, &item.fid)? {
                    pending.push((path.clone(), child));
                }
            } else {
                files.push((dir, item));
            }
        }
        files.sort_by(|a, b| (a.0.as_str(), a.1.name.as_str()).cmp(&(b.0.as_str(), b.1.name.as_str())));
        Ok((top, files))
    }

    /// True when the cookie is a working signed-in session (one cheap call to
    /// the user's own drive root).
    pub fn signed_in(&self) -> bool {
        if self.cookie().is_empty() {
            return false;
        }
        let query = [("pdir_fid", "0".to_owned()), ("_page", "1".to_owned()), ("_size", "1".to_owned())];
        self.get("file/sort", &query).is_ok_and(|value| value.get("code").and_then(Value::as_i64) == Some(0))
    }

    /// Creates a folder in the user's drive by path and returns its fid.
    pub fn make_folder(&self, dir_path: &str) -> Result<String, String> {
        let value = self.post(
            "file",
            json!({ "pdir_fid": "0", "file_name": "", "dir_path": dir_path, "dir_init_lock": false }),
        )?;
        let value = checked(self.flavor.name, "创建临时文件夹", value)?;
        let fid = value.pointer("/data/fid").map(|fid| field_str(&json!({ "fid": fid }), "fid")).unwrap_or_default();
        if fid.is_empty() {
            return Err(format!("{}没有返回临时文件夹", self.flavor.name));
        }
        Ok(fid)
    }

    /// Saves the share's top items into `to_pdir_fid` and waits for the task.
    pub fn save_share(&self, pwd_id: &str, stoken: &str, top: &[QuarkItem], to_pdir_fid: &str) -> Result<(), String> {
        let value = self.post(
            "share/sharepage/save",
            json!({
                "fid_list": top.iter().map(|item| item.fid.clone()).collect::<Vec<_>>(),
                "fid_token_list": top.iter().map(|item| item.share_fid_token.clone()).collect::<Vec<_>>(),
                "to_pdir_fid": to_pdir_fid,
                "pwd_id": pwd_id,
                "stoken": stoken,
                "pdir_fid": "0",
                "scene": "link",
            }),
        )?;
        let value = checked(self.flavor.name, "转存到网盘", value)?;
        let task_id = value.pointer("/data/task_id").and_then(Value::as_str).unwrap_or("").to_owned();
        if task_id.is_empty() {
            return Err(format!("{}没有返回转存任务", self.flavor.name));
        }
        let deadline = Instant::now() + Duration::from_secs(300);
        for retry in 0.. {
            let value = checked(self.flavor.name, 
                "查询转存进度",
                self.get("task", &[("task_id", task_id.clone()), ("retry_index", retry.to_string())])?,
            )?;
            if value.pointer("/data/status").and_then(Value::as_i64) == Some(2) {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(format!("{}转存 5 分钟内没有完成", self.flavor.name));
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        unreachable!()
    }

    /// Items of a folder in the user's drive, all pages.
    pub fn drive_list(&self, pdir_fid: &str) -> Result<Vec<QuarkItem>, String> {
        let mut items = Vec::new();
        for page in 1..=200 {
            let value = checked(self.flavor.name, 
                "读取网盘文件夹",
                self.get(
                    "file/sort",
                    &[
                        ("pdir_fid", pdir_fid.to_owned()),
                        ("_page", page.to_string()),
                        ("_size", "100".to_owned()),
                        ("_fetch_total", "1".to_owned()),
                        ("_sort", "file_type:asc,file_name:asc".to_owned()),
                    ],
                )?,
            )?;
            let list = value.pointer("/data/list").and_then(Value::as_array).cloned().unwrap_or_default();
            let total = value.pointer("/metadata/_total").and_then(Value::as_u64).unwrap_or(0) as usize;
            items.extend(list.iter().filter_map(parse_item));
            if list.is_empty() || items.len() >= total || items.len() > MAX_ITEMS {
                break;
            }
        }
        Ok(items)
    }

    /// Every file under a drive folder, with its path relative to that folder.
    pub fn drive_files(&self, root_fid: &str) -> Result<Vec<(String, QuarkItem)>, String> {
        let mut files = Vec::new();
        let mut pending = vec![(String::new(), root_fid.to_owned())];
        while let Some((dir, fid)) = pending.pop() {
            for item in self.drive_list(&fid)? {
                if item.is_dir {
                    let path = if dir.is_empty() { item.name.clone() } else { format!("{dir}/{}", item.name) };
                    pending.push((path, item.fid.clone()));
                } else {
                    files.push((dir.clone(), item));
                }
                if files.len() > MAX_ITEMS {
                    return Err("网盘文件夹超过 10000 个文件".to_owned());
                }
            }
        }
        Ok(files)
    }

    /// Download URLs for drive files (fid → url). Download them with
    /// [`Self::cookie`] read afterwards: Quark sets download cookies here.
    pub fn download_links(&self, fids: &[String]) -> Result<Vec<(String, String)>, String> {
        let mut links = Vec::new();
        for chunk in fids.chunks(20) {
            let value = checked(self.flavor.name, "获取下载地址", self.post("file/download", json!({ "fids": chunk }))?)?;
            for item in value.get("data").and_then(Value::as_array).cloned().unwrap_or_default() {
                let fid = field_str(&item, "fid");
                let url = field_str(&item, "download_url");
                if !fid.is_empty() && url.starts_with("https://") {
                    links.push((fid, url));
                }
            }
        }
        Ok(links)
    }

    /// Moves a drive folder to the recycle bin.
    pub fn delete(&self, fid: &str) -> Result<(), String> {
        let value = self.post("file/delete", json!({ "action_type": 2, "filelist": [fid], "exclude_fids": [] }))?;
        checked(self.flavor.name, "删除临时文件夹", value).map(|_| ())
    }
}

/// Temporary folders AI-OS creates in the user's drive live under this one.
pub const TEMP_ROOT: &str = "/AI-OS 临时下载";

/// The whole share with the user's signed-in session: save it into a fresh
/// temporary folder in their drive, download every file into `destination`,
/// verify sizes and SHA-256, then delete the temporary folder (to Quark's
/// recycle bin) once everything arrived. Same result shape as Baidu.
pub fn download_share(
    flavor: &'static Flavor,
    cookie: &str,
    pwd_id: &str,
    passcode: &str,
    destination: &std::path::Path,
    log: impl Fn(&str),
) -> Result<Value, String> {
    use crate::cloud_drive_core::{DownloadDescriptor, DownloadHeader};
    let session = QuarkSession::new(flavor, cookie)?;
    log("open share");
    let stoken = session.share_token(pwd_id, passcode)?;
    let (top, expected) = session.share_files(pwd_id, &stoken)?;
    if expected.is_empty() {
        return Err("分享里没有可下载的文件".to_owned());
    }
    let folder_path = format!("{TEMP_ROOT}/{pwd_id}-{}", chrono::Utc::now().timestamp());
    log("create temporary folder");
    let folder_fid = session.make_folder(&folder_path)?;
    log("save to drive");
    session.save_share(pwd_id, &stoken, &top, &folder_fid)?;
    log("list saved files");
    let saved = session.drive_files(&folder_fid)?;
    let key = |dir: &str, name: &str| if dir.is_empty() { name.to_owned() } else { format!("{dir}/{name}") };
    let saved_fid = |path: &str| saved.iter().find(|(dir, item)| key(dir, &item.name) == path).map(|(_, item)| item.fid.clone());
    log("get download links");
    let fids: Vec<String> = expected.iter().filter_map(|(dir, item)| saved_fid(&key(dir, &item.name))).collect();
    let links = session.download_links(&fids)?;
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .build()
        .map_err(|error| format!("无法创建下载客户端：{error}"))?;

    let mut receipts = Vec::new();
    let mut skipped = Vec::new();
    // Progress: how many files (and bytes) still have to arrive.
    let todo: Vec<u64> = expected
        .iter()
        .filter(|(dir, item)| !crate::cloud_drive::already_downloaded(destination, dir, &item.name, item.size))
        .map(|(_, item)| item.size)
        .collect();
    log(&format!("plan {} files {} bytes", todo.len(), todo.iter().sum::<u64>()));
    let mut index = 0;
    for (dir, item) in &expected {
        let shown = key(dir, &item.name);
        let skip = |reason: String| json!({ "name": shown, "bytes": item.size, "reason": reason });
        let Some(directory) = crate::cloud_drive::safe_relative_dir(dir) else {
            skipped.push(skip("文件夹名称不安全".to_owned()));
            continue;
        };
        let directory = destination.join(directory);
        // The same link sent again: an intact earlier copy counts as done.
        let existing = directory.join(&item.name);
        if existing.metadata().is_ok_and(|metadata| metadata.is_file() && metadata.len() == item.size) {
            receipts.push(json!({
                "provider": flavor.key, "path": existing.to_string_lossy(), "bytes": item.size,
                "sha256": crate::cloud_drive::sha256_hex(&existing)?, "attempts": 0, "integrityVerified": true,
            }));
            continue;
        }
        let Some(url) = saved_fid(&shown).and_then(|fid| links.iter().find(|(id, _)| *id == fid).map(|(_, url)| url.clone())) else {
            skipped.push(skip(format!("{}没有给出这个文件的下载地址", flavor.name)));
            continue;
        };
        index += 1;
        log(&format!("download {index}/{} {} bytes", todo.len(), item.size));
        let descriptor = DownloadDescriptor {
            provider: flavor.provider,
            url,
            file_name: item.name.clone(),
            headers: vec![
                DownloadHeader { name: "User-Agent".to_owned(), value: flavor.user_agent.to_owned() },
                DownloadHeader { name: "Referer".to_owned(), value: flavor.referer.to_owned() },
                DownloadHeader { name: "Cookie".to_owned(), value: session.cookie() },
            ],
            expected_size: Some(item.size),
            expected_sha256: None,
        };
        match crate::cloud_drive_core::download_file_with_client(&client, &descriptor, &directory, 3, |_| {}) {
            Ok(receipt) => receipts.push(serde_json::to_value(receipt).map_err(|error| error.to_string())?),
            Err(error) => skipped.push(skip(error)),
        }
    }
    let deleted = skipped.is_empty() && {
        log("delete temporary folder");
        session.delete(&folder_fid).is_ok()
    };
    Ok(json!({
        "receipts": receipts,
        "skipped": skipped,
        "viaDrive": receipts.len(),
        "driveFolder": folder_path,
        "driveFolderDeleted": deleted,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_parse_from_quark_shapes() {
        let file = parse_item(&json!({"fid": "abc", "file_name": "a.zip", "size": 10623585, "dir": false, "share_fid_token": "t"})).unwrap();
        assert_eq!((file.fid.as_str(), file.size, file.is_dir, file.share_fid_token.as_str()), ("abc", 10623585, false, "t"));
        let folder = parse_item(&json!({"fid": "d", "file_name": "Aimv001", "file_type": 0})).unwrap();
        assert!(folder.is_dir);
        assert!(parse_item(&json!({"file_name": "no fid"})).is_none());
    }

    #[test]
    fn quark_errors_carry_code_and_message() {
        assert!(checked("夸克", "x", json!({"code": 0, "data": {}})).is_ok());
        let error = checked("夸克", "转存到网盘", json!({"code": 41006, "message": "分享不存在"})).unwrap_err();
        assert_eq!(error, "夸克转存到网盘失败（code 41006，分享不存在）");
    }

    #[test]
    #[ignore = "Requires internet access"]
    fn reads_a_real_public_share_anonymously() {
        // Public share of the open-source GitHub Store app (found 2026-10-18).
        let session = QuarkSession::new(&QUARK, "").unwrap();
        let stoken = session.share_token("97e0db6659eb", "").unwrap();
        let (top, files) = session.share_files("97e0db6659eb", &stoken).unwrap();
        assert_eq!(top.len(), 1);
        assert_eq!(files.len(), 1);
        assert!(files[0].1.size > 1_000_000);
    }

    #[test]
    #[ignore = "Requires internet access"]
    fn reads_a_real_public_uc_share_anonymously() {
        // OK影视 share listed in github.com/youhunwl/TVAPP (found 2026-10-18).
        let session = QuarkSession::new(&UC, "").unwrap();
        let stoken = session.share_token("0be7077c38654", "").unwrap();
        let (top, files) = session.share_files("0be7077c38654", &stoken).unwrap();
        assert!(!top.is_empty() && !files.is_empty());
        assert!(files.iter().all(|(_, item)| !item.fid.is_empty()));
        // The same share is not found under Quark's product code.
        assert!(QuarkSession::new(&QUARK, "").unwrap().share_token("0be7077c38654", "").is_err());
    }
}
