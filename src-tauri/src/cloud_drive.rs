//! Built-in cloud-drive download (Beta 5).
//!
//! Owner decision 2026-10-17: the user sends a share link and AI-OS downloads
//! the files, with nothing to install or confirm. LinkSwift is not forked,
//! bundled or republished; AI-OS uses Baidu's own routes:
//! - Baidu's official CLI `bdpan` when it is installed and signed in (save to
//!   the user's drive, download, remove) — the only route that delivers large
//!   files;
//! - otherwise the signed-in web flow inside a hidden WebView, which can only
//!   download what Baidu allows directly from a share (small files).
//! The `ai-os-cloud-drive-mcp` executable ships inside AI OS.app for link
//! recognition.

use crate::{cloud_drive_baidu, cloud_drive_core::ShareLink, mcp_runtime};
use chrono::Utc;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use url::Url;

const EXECUTABLE_NAME: &str = "ai-os-cloud-drive-mcp";
const BAIDU_AUTH_WINDOW_LABEL: &str = "cloud-drive-baidu-authorization";

#[tauri::command]
pub async fn call_cloud_drive_mcp(
    tool_name: String,
    arguments: Value,
) -> Result<mcp_runtime::McpResponse, String> {
    if !matches!(
        tool_name.as_str(),
        "inspect_share_link" | "resolve_baidu_share" | "select_share_files"
    ) {
        return Err("The requested AI-OS Cloud Drive MCP tool is not allowed.".to_owned());
    }
    let command = cloud_drive_mcp_command()?;
    tauri::async_runtime::spawn_blocking(move || {
        mcp_runtime::call_mcp_tool(command, Vec::new(), tool_name, arguments)
    })
    .await
    .map_err(|error| format!("Cloud Drive MCP task failed: {error}"))?
}

/// AI-OS's own persistent store for Baidu sign-in, isolated from the app's
/// main WebView and from any browser.
const BAIDU_DATA_STORE: [u8; 16] = *b"AI-OS-baidu-auth";
/// How long AI-OS waits for the user to finish signing in.
const BAIDU_LOGIN_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);
/// How long a saved sign-in may take to appear before the window is shown.
const BAIDU_SAVED_LOGIN_GRACE: std::time::Duration = std::time::Duration::from_secs(3);

/// The memory-only authorization header once the user is signed in to Baidu
/// (BDUSS present), or None while they are not.
fn baidu_signed_in_cookie(window: &tauri::WebviewWindow) -> Result<Option<String>, String> {
    // Not cookies_for_url: wry matches the cookie domain to the URL host
    // exactly, so BDUSS (domain .baidu.com) never matched pan.baidu.com and a
    // completed sign-in was never seen. Read the store and match Baidu domains.
    let cookies = window
        .cookies()
        .map_err(|error| format!("Unable to read the Baidu sign-in: {error}"))?
        .into_iter()
        .filter(|cookie| cookie.domain().is_some_and(is_baidu_cookie_domain))
        .map(|cookie| (cookie.name().to_owned(), cookie.value().to_owned()))
        .collect::<Vec<_>>();
    Ok(baidu_authorization_header(&cookies))
}

fn is_baidu_cookie_domain(domain: &str) -> bool {
    let domain = domain.trim_start_matches('.').to_ascii_lowercase();
    domain == "baidu.com" || domain.ends_with(".baidu.com")
}

/// The allow-listed authorization cookies, once BDUSS shows the user is signed
/// in; one value per name.
fn baidu_authorization_header(cookies: &[(String, String)]) -> Option<String> {
    let signed_in = cookies
        .iter()
        .any(|(name, value)| matches!(name.as_str(), "BDUSS" | "BDUSS_BFESS") && !value.is_empty());
    if !signed_in {
        return None;
    }
    let mut seen = std::collections::HashSet::new();
    Some(
        cookies
            .iter()
            .filter(|(name, value)| {
                cloud_drive_baidu::is_authorization_cookie_name(name)
                    && !value.is_empty()
                    && seen.insert(name.clone())
            })
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; "),
    )
}

/// Path the in-window script navigates to in order to hand its result back.
/// The navigation is intercepted and never happens.
const BAIDU_RESULT_PATH: &str = "/__aios_result__";
/// Path the in-window script navigates to for progress lines (also intercepted).
const BAIDU_PROGRESS_PATH: &str = "/__aios_progress__";

/// Runs inside the signed-in Baidu page, exactly as the Baidu web client
/// would: verify the extraction code, list every file (folders included),
/// fetch the download signature and each file's download URL. The browser
/// owns redirects and cookies, which a native client kept getting wrong
/// (owner acceptance 2026-10-17). Placeholders: __SURL__, __PWD__, __RESULT__.
const BAIDU_SHARE_SCRIPT: &str = r#"(async () => {
  const report = (payload) => {
    location.href = "https://pan.baidu.com__RESULT__?d=" + encodeURIComponent(JSON.stringify(payload));
  };
  // Progress lines for ai-os.log: names and counts only, never URLs or tokens.
  const started = Date.now();
  const step = (stage) => {
    location.href = "https://pan.baidu.com__PROGRESS__?s=" + encodeURIComponent(`${Math.round((Date.now() - started) / 1000)}s ${stage}`);
  };
  try {
    step("script started");
    const surl = __SURL__, pwd = __PWD__;
    const q = (extra) => new URLSearchParams({ channel: "chunlei", web: "1", app_id: "250528", clienttype: "0", ...extra }).toString();
    const getJson = async (path, params) => (await fetch(`/${path}?${q(params)}`, { credentials: "include" })).json();
    const postJson = async (path, params, body) => (await fetch(`/${path}?${q(params)}`, {
      method: "POST", credentials: "include",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams(body),
    })).json();
    const verify = await postJson("share/verify", { surl, t: String(Date.now()) }, { pwd, vcode: "", vcode_str: "" });
    if (verify.errno !== 0) return report({ error: `提取码验证失败（errno ${verify.errno}）` });
    const sekey = decodeURIComponent(verify.randsk);
    step("extraction code verified");
    const root = await getJson("share/list", { shorturl: surl, root: "1", page: "1", num: "1000" });
    if (root.errno !== 0) return report({ error: `读取分享文件失败（errno ${root.errno}）` });
    const shareid = root.share_id, uk = root.uk;
    const files = [];
    const walk = async (items, dir) => {
      for (const item of items) {
        if (String(item.isdir) === "1") {
          const sub = dir ? `${dir}/${item.server_filename}` : item.server_filename;
          for (let page = 1; ; page += 1) {
            const listed = await getJson("share/list", { uk: String(uk), shareid: String(shareid), dir: item.path, page: String(page), num: "1000", order: "name", desc: "0", showempty: "0" });
            if (listed.errno !== 0) throw new Error(`读取文件夹失败（errno ${listed.errno}）`);
            await walk(listed.list || [], sub);
            if ((listed.list || []).length < 1000) break;
          }
        } else {
          files.push({ dir, name: item.server_filename, size: Number(item.size), fsId: item.fs_id });
        }
        if (files.length > 10000) throw new Error("分享超过 10000 个文件");
      }
    };
    await walk(root.list || [], "");
    step(`listed ${files.length} file(s)`);
    if (files.length === 0) return report({ error: "分享里没有可下载的文件" });
    const tpl = await getJson("share/tplconfig", { shareid: String(shareid), uk: String(uk), fields: "sign,timestamp", view_mode: "1" });
    if (tpl.errno !== 0) return report({ error: `获取下载签名失败（errno ${tpl.errno}）` });
    // Parameters the Baidu web client sends when the page provides them.
    const baiduId = (document.cookie.match(/(?:^|; )BAIDUID=([^;]+)/) || [])[1] || "";
    const bdstoken = (window.locals && (window.locals.bdstoken?.value ?? window.locals.get?.("bdstoken"))) || "";
    const extraParams = {};
    if (bdstoken) extraParams.bdstoken = String(bdstoken);
    if (baiduId) extraParams.logid = btoa(baiduId);
    if (window.jsToken) extraParams.jsToken = String(window.jsToken);
    for (const file of files) {
      const resolved = await postJson("api/sharedownload", { sign: tpl.data.sign, timestamp: String(tpl.data.timestamp), ...extraParams }, {
        encrypt: "0", product: "share", uk: String(uk), primaryid: String(shareid),
        fid_list: JSON.stringify([file.fsId]), extra: JSON.stringify({ sekey }),
      });
      // errno 0 with `list` as a string is Baidu refusing a direct share
      // download because the file is too large: it must first be saved to the
      // user's own drive (the same rule LinkSwift reports). Other files go on.
      if (resolved.errno === 0 && typeof resolved.list === "string") {
        file.skipped = "百度要求这个文件先保存到网盘才能下载（文件较大）";
        file.needsDrive = true;
        continue;
      }
      if (resolved.errno !== 0 || !Array.isArray(resolved.list) || !resolved.list[0] || !resolved.list[0].dlink) {
        file.skipped = `百度没有给出下载地址（errno ${resolved.errno}${resolved.show_msg ? "，" + resolved.show_msg : ""}）`;
        continue;
      }
      file.dlink = resolved.list[0].dlink;
    }
    step(`direct links ${files.filter((f) => f.dlink).length}, need drive ${files.filter((f) => f.needsDrive).length}`);
    // Owner decision 2026-10-17 (option 1): files Baidu refuses as a direct
    // share download are saved into a temporary folder in the user's drive,
    // downloaded from there, and the folder is deleted afterwards (by Rust,
    // once every download is verified). Only that folder is ever touched.
    let driveFolder = null;
    const large = files.filter((file) => file.needsDrive);
    if (large.length > 0) {
      const fail = (reason) => { for (const file of large) file.skipped = reason; };
      const vars = await getJson("api/gettemplatevariable", { fields: JSON.stringify(["bdstoken"]) });
      const token = (vars && vars.result && vars.result.bdstoken) || bdstoken;
      if (!token) {
        fail("需要转存到网盘，但没有拿到网盘操作令牌");
      } else {
        driveFolder = `/AI-OS 临时下载/${shareid}-${Date.now()}`;
        step("creating drive folder");
        const created = await postJson("api/create", { a: "commit", bdstoken: token }, { path: driveFolder, isdir: "1", block_list: "[]" });
        const transfer = created.errno === 0
          ? await postJson("share/transfer", { shareid: String(shareid), from: String(uk), sekey, ondup: "newcopy", async: "1", bdstoken: token }, {
              fsidlist: JSON.stringify(large.map((file) => file.fsId)), path: driveFolder,
            })
          : created;
        step(`create errno ${created.errno}, transfer errno ${transfer.errno}`);
        if (transfer.errno !== 0) {
          fail(`转存到网盘失败（errno ${transfer.errno}${transfer.show_msg ? "，" + transfer.show_msg : ""}）`);
        } else {
          // Transfers may finish asynchronously; poll until the files appear.
          // No setTimeout: macOS suspends timers in the hidden window, which
          // stalled the script (owner acceptance 2026-10-18). Each request
          // takes real time, so repeated requests pace themselves.
          let saved = [];
          const pollUntil = Date.now() + 90000;
          while (Date.now() < pollUntil) {
            const listed = await getJson("api/list", { dir: driveFolder, num: "1000", order: "name", desc: "0", bdstoken: token });
            saved = (listed.list || []).filter((entry) => String(entry.isdir) !== "1");
            if (saved.length >= large.length) break;
          }
          step(`drive folder holds ${saved.length} of ${large.length}`);
          const unused = [...saved];
          for (const file of large) {
            const index = unused.findIndex((entry) => Number(entry.size) === file.size && entry.server_filename === file.name);
            const fallback = index >= 0 ? index : unused.findIndex((entry) => Number(entry.size) === file.size);
            if (fallback < 0) { file.skipped = "转存后在网盘里没有找到这个文件"; continue; }
            file.drivePath = unused.splice(fallback, 1)[0].path;
          }
          const paths = large.filter((file) => file.drivePath).map((file) => file.drivePath);
          if (paths.length > 0) {
            const metas = await getJson("api/filemetas", { target: JSON.stringify(paths), dlink: "1", bdstoken: token });
            for (const file of large.filter((item) => item.drivePath)) {
              const info = (metas.info || []).find((item) => item.path === file.drivePath);
              step(`filemetas errno ${metas.errno}, dlink ${Boolean(info && info.dlink)}`);
              if (metas.errno === 0 && info && info.dlink) {
                file.dlink = info.dlink;
                file.skipped = undefined;
                file.viaDrive = true;
              } else {
                file.skipped = `从网盘获取下载地址失败（errno ${metas.errno}）`;
              }
            }
          }
        }
      }
    }
    step("reporting");
    report({ files, userAgent: navigator.userAgent, driveFolder });
  } catch (error) {
    report({ error: String(error && error.message || error) });
  }
})();"#;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BaiduScriptFile {
    dir: String,
    name: String,
    size: u64,
    #[serde(default)]
    dlink: Option<String>,
    #[serde(default)]
    skipped: Option<String>,
    #[serde(default)]
    via_drive: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BaiduScriptResult {
    #[serde(default)]
    files: Vec<BaiduScriptFile>,
    #[serde(default)]
    drive_folder: Option<String>,
    #[serde(default)]
    user_agent: String,
    #[serde(default)]
    error: Option<String>,
}

/// A share folder path made safe to join under the destination.
fn safe_relative_dir(dir: &str) -> Option<PathBuf> {
    let mut path = PathBuf::new();
    for part in dir.split('/').map(str::trim).filter(|part| !part.is_empty()) {
        if part == "." || part == ".." || part.contains('\\') || part.chars().any(char::is_control) {
            return None;
        }
        path.push(part);
    }
    Some(path)
}

/// Every Baidu cookie in AI-OS's store, one per name (the pan.baidu.com one
/// wins over a parent-domain duplicate), as a request header.
fn baidu_cookie_header(window: &tauri::WebviewWindow) -> Result<String, String> {
    let mut chosen: Vec<(String, String, bool)> = Vec::new();
    for cookie in window
        .cookies()
        .map_err(|error| format!("Unable to read the Baidu sign-in: {error}"))?
    {
        let Some(domain) = cookie.domain() else { continue };
        if !is_baidu_cookie_domain(domain) || cookie.value().is_empty() {
            continue;
        }
        let specific = domain.trim_start_matches('.') == "pan.baidu.com";
        match chosen.iter_mut().find(|(name, _, _)| name == cookie.name()) {
            Some(existing) if specific || !existing.2 => {
                existing.1 = cookie.value().to_owned();
                existing.2 = specific;
            }
            Some(_) => {}
            None => chosen.push((cookie.name().to_owned(), cookie.value().to_owned(), specific)),
        }
    }
    Ok(chosen
        .into_iter()
        .map(|(name, value, _)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("; "))
}

/// Baidu's official command-line client, when installed. It is the route
/// Baidu supports for share downloads (save to the user's drive, download,
/// remove), and the only one that delivered large files (owner acceptance
/// 2026-10-18: the web route is refused with 403 for drive files).
fn bdpan_executable() -> Option<PathBuf> {
    let home = dirs::home_dir()?;
    [
        home.join(".local/bin/bdpan"),
        PathBuf::from("/opt/homebrew/bin/bdpan"),
        PathBuf::from("/usr/local/bin/bdpan"),
    ]
    .into_iter()
    .find(|path| path.is_file())
}

fn bdpan_json(bin: &Path, args: &[&str]) -> Result<Value, String> {
    let output = std::process::Command::new(bin)
        .args(args)
        .args(["--json", "--no-check-update", "--agentname", "AI-OS"])
        .output()
        .map_err(|error| format!("无法运行百度网盘命令行工具：{error}"))?;
    serde_json::from_slice(&output.stdout).map_err(|_| {
        let detail: String = String::from_utf8_lossy(&output.stderr).chars().take(200).collect();
        format!("百度网盘命令行工具返回了无法读取的结果。{detail}")
    })
}

fn bdpan_signed_in(bin: &Path) -> bool {
    let Ok(value) = bdpan_json(bin, &["whoami"]) else { return false };
    let info = value.get("data").filter(|data| data.is_object()).unwrap_or(&value);
    info.get("authenticated").and_then(Value::as_bool).unwrap_or(false)
        && info.get("has_valid_token").and_then(Value::as_bool).unwrap_or(false)
}

fn sha256_hex(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = fs::File::open(path).map_err(|error| format!("无法校验下载的文件：{error}"))?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|error| format!("无法校验下载的文件：{error}"))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// Expected files of a share (relative path → size), read anonymously.
fn baidu_share_expected_files(share: &ShareLink) -> Result<Vec<(String, u64)>, String> {
    let resolved = cloud_drive_baidu::BaiduResolvedShare::resolve(share.clone())?;
    fn walk(entries: &[crate::cloud_drive_core::ShareEntry], dir: &str, out: &mut Vec<(String, u64)>) {
        for entry in entries {
            let path = if dir.is_empty() { entry.name.clone() } else { format!("{dir}/{}", entry.name) };
            match entry.kind {
                crate::cloud_drive_core::ShareEntryKind::File => out.push((path, entry.size.unwrap_or(0))),
                crate::cloud_drive_core::ShareEntryKind::Folder => walk(&entry.children, &path, out),
            }
        }
    }
    let mut out = Vec::new();
    walk(resolved.entries(), "", &mut out);
    Ok(out)
}

/// The whole share through bdpan: save into "AI-OS 临时下载" inside bdpan's
/// own app folder, download, verify each file against the share listing,
/// then remove the temporary folder.
fn download_baidu_share_with_bdpan(bin: &Path, share: &ShareLink, code: &str) -> Result<Value, String> {
    let expected = baidu_share_expected_files(share)?;
    if expected.is_empty() {
        return Err("分享里没有可下载的文件".to_owned());
    }
    let destination = PathBuf::from(default_download_directory(&share.share_id)?);
    fs::create_dir_all(&destination).map_err(|error| format!("无法创建下载目录：{error}"))?;
    let transfer_dir = format!("AI-OS 临时下载/{}-{}", share.share_id, Utc::now().timestamp());
    let destination_text = destination.to_string_lossy().into_owned();
    let result = bdpan_json(
        bin,
        &["download", &share.canonical_url, &destination_text, "-p", code, "-t", &transfer_dir],
    )?;
    let error = result.get("error").and_then(Value::as_str).unwrap_or("");
    let failed = !error.is_empty() || result.get("code").and_then(Value::as_i64).is_some_and(|code| code != 0);

    let mut receipts = Vec::new();
    let mut skipped = Vec::new();
    for (relative, size) in &expected {
        let Some(relative_path) = safe_relative_dir(relative) else {
            skipped.push(serde_json::json!({ "name": relative, "bytes": size, "reason": "文件名不安全" }));
            continue;
        };
        let path = destination.join(relative_path);
        match fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => receipts.push(serde_json::json!({
                "provider": "baidu",
                "path": path.to_string_lossy(),
                "bytes": metadata.len(),
                "sha256": sha256_hex(&path)?,
                "attempts": 1,
                "integrityVerified": metadata.len() == *size,
            })),
            _ => skipped.push(serde_json::json!({
                "name": relative,
                "bytes": size,
                "reason": if failed { format!("百度网盘命令行工具下载失败：{error}") } else { "下载后没有找到这个文件".to_owned() },
            })),
        }
    }
    // Remove the temporary copy only when everything arrived intact.
    let complete = skipped.is_empty()
        && receipts.iter().all(|receipt| receipt["integrityVerified"].as_bool() == Some(true));
    let deleted = complete
        && bdpan_json(bin, &["rm", "-f", &transfer_dir])
            .ok()
            .and_then(|value| value.get("status").and_then(Value::as_str).map(|status| status == "ok"))
            .unwrap_or(false);
    Ok(serde_json::json!({
        "receipts": receipts,
        "skipped": skipped,
        "viaDrive": receipts.len(),
        "driveFolder": format!("我的应用数据/bdpan/{transfer_dir}"),
        "driveFolderDeleted": deleted,
    }))
}

/// Owner decision 2026-10-17: downloading is automatic. A share link is
/// downloaded in full with no confirmation. Baidu sign-in is kept in AI-OS's
/// own store, so the window appears only when the user is not signed in. The
/// signed-in (hidden) window then performs Baidu's own web flow to resolve the
/// files, and AI-OS downloads them with that session and verifies them.
#[tauri::command]
pub async fn download_baidu_share_automatically(
    app: tauri::AppHandle,
    url: String,
    extraction_code: String,
) -> Result<Value, String> {
    let share = ShareLink::parse(&url, Some(&extraction_code))?;
    if share.provider != crate::cloud_drive_core::CloudDriveProvider::Baidu {
        return Err("Automatic sign-in is only available for Baidu shares.".to_owned());
    }
    // Preferred: Baidu's official CLI, when installed and signed in.
    if let Some(bin) = bdpan_executable() {
        let bdpan_share = share.clone();
        let code = extraction_code.clone();
        let outcome = tauri::async_runtime::spawn_blocking(move || {
            if !bdpan_signed_in(&bin) {
                return None;
            }
            crate::logs::append_ai_os_diagnostic("[CLOUD_DRIVE] baidu via bdpan");
            Some(download_baidu_share_with_bdpan(&bin, &bdpan_share, &code))
        })
        .await
        .map_err(|error| format!("Cloud Drive download task failed: {error}"))?;
        if let Some(result) = outcome {
            return result;
        }
    }
    let surl = share
        .share_id
        .strip_prefix('1')
        .unwrap_or(&share.share_id)
        .to_owned();
    if let Some(existing) = app.get_webview_window(BAIDU_AUTH_WINDOW_LABEL) {
        let _ = existing.close();
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }

    let (result_tx, mut result_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let last_stage = std::sync::Arc::new(std::sync::Mutex::new(String::from("not started")));
    let stage_writer = last_stage.clone();
    let (load_tx, mut load_rx) = tokio::sync::mpsc::unbounded_channel::<Url>();
    let page = Url::parse("https://passport.baidu.com/v2/?login&u=https%3A%2F%2Fpan.baidu.com%2F")
        .expect("static URL");
    let window = WebviewWindowBuilder::new(&app, BAIDU_AUTH_WINDOW_LABEL, WebviewUrl::External(page))
        .title("登录百度网盘 — 只需登录，AI-OS 会自动下载")
        .inner_size(960.0, 720.0)
        .visible(false)
        .data_store_identifier(BAIDU_DATA_STORE)
        .on_navigation(move |target| {
            if target.host_str() == Some("pan.baidu.com") && target.path() == BAIDU_PROGRESS_PATH {
                if let Some((_, stage)) = target.query_pairs().find(|(key, _)| key == "s") {
                    crate::logs::append_ai_os_diagnostic(&format!("[CLOUD_DRIVE] baidu {stage}"));
                    if let Ok(mut last) = stage_writer.lock() {
                        *last = stage.into_owned();
                    }
                }
                return false;
            }
            if target.host_str() == Some("pan.baidu.com") && target.path() == BAIDU_RESULT_PATH {
                if let Some((_, payload)) = target.query_pairs().find(|(key, _)| key == "d") {
                    let _ = result_tx.send(payload.into_owned());
                }
                return false;
            }
            target.scheme() == "https"
                && target
                    .host_str()
                    .is_some_and(|host| host == "baidu.com" || host.ends_with(".baidu.com"))
        })
        .on_page_load(move |_, payload| {
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                let _ = load_tx.send(payload.url().clone());
            }
        })
        .build()
        .map_err(|error| format!("Unable to open the Baidu sign-in window: {error}"))?;

    // 1. Sign-in: silent when AI-OS's store already holds it.
    let started = std::time::Instant::now();
    let mut shown = false;
    loop {
        if app.get_webview_window(BAIDU_AUTH_WINDOW_LABEL).is_none() {
            return Err("百度登录窗口被关闭，下载已取消。".to_owned());
        }
        if baidu_signed_in_cookie(&window)?.is_some() {
            break;
        }
        if !shown && started.elapsed() >= BAIDU_SAVED_LOGIN_GRACE {
            let _ = window.show();
            let _ = window.set_focus();
            shown = true;
        }
        if started.elapsed() >= BAIDU_LOGIN_TIMEOUT {
            let _ = window.close();
            return Err("等了 10 分钟仍未完成百度登录，下载已取消。".to_owned());
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
    // Not hide(): macOS suspends the page of an ordered-out window, which
    // stalled the longer save-to-drive flow (owner acceptance 2026-10-18).
    // The window stays "visible" but outside every screen and never focused.
    let _ = window.set_position(tauri::PhysicalPosition::new(-32000, -32000));
    let _ = window.set_skip_taskbar(true);
    let _ = window.show();
    crate::logs::append_ai_os_diagnostic("[CLOUD_DRIVE] baidu signed in; resolving share off-screen");

    // 2. Resolve the share inside the signed-in page.
    while load_rx.try_recv().is_ok() {}
    let init = Url::parse(&format!("https://pan.baidu.com/share/init?surl={surl}"))
        .map_err(|_| "Baidu share URL is invalid.".to_owned())?;
    window
        .navigate(init)
        .map_err(|error| format!("Unable to open the Baidu share: {error}"))?;
    let loaded = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        while let Some(url) = load_rx.recv().await {
            if url.host_str() == Some("pan.baidu.com") {
                return true;
            }
        }
        false
    })
    .await
    .unwrap_or(false);
    if !loaded {
        let _ = window.close();
        return Err("打开百度分享页面超时。".to_owned());
    }
    let script = BAIDU_SHARE_SCRIPT
        .replace("__SURL__", &serde_json::to_string(&surl).expect("string"))
        .replace("__PWD__", &serde_json::to_string(&extraction_code).expect("string"))
        .replace("__RESULT__", BAIDU_RESULT_PATH)
        .replace("__PROGRESS__", BAIDU_PROGRESS_PATH);
    window
        .eval(&script)
        .map_err(|error| format!("Unable to resolve the Baidu share: {error}"))?;
    let payload = tokio::time::timeout(std::time::Duration::from_secs(180), result_rx.recv())
        .await
        .ok()
        .flatten();
    let cookie_header = baidu_cookie_header(&window);
    let payload = match payload {
        Some(payload) => payload,
        None => {
            let _ = window.close();
            let stage = last_stage.lock().map(|value| value.clone()).unwrap_or_default();
            return Err(format!("百度没有在 3 分钟内返回分享内容（最后进度：{stage}）。"));
        }
    };
    let resolved: BaiduScriptResult = match serde_json::from_str(&payload) {
        Ok(resolved) => resolved,
        Err(_) => {
            let _ = window.close();
            return Err("百度返回的分享内容无法读取。".to_owned());
        }
    };
    if let Some(error) = resolved.error {
        let _ = window.close();
        return Err(error);
    }
    let cookie_header = match cookie_header {
        Ok(header) => header,
        Err(error) => {
            let _ = window.close();
            return Err(error);
        }
    };

    // 3. Download with that session; size and SHA-256 are verified.
    let destination = PathBuf::from(default_download_directory(&share.share_id)?);
    let drive_folder = resolved.drive_folder.clone();
    let downloaded = tauri::async_runtime::spawn_blocking(move || {
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(20))
            .build()
            .map_err(|error| format!("Unable to create the download client: {error}"))?;
        let mut receipts = Vec::new();
        let mut skipped = Vec::new();
        let mut via_drive = 0_usize;
        let mut via_drive_failed = 0_usize;
        for file in resolved.files {
            let shown_name = if file.dir.is_empty() {
                file.name.clone()
            } else {
                format!("{}/{}", file.dir, file.name)
            };
            let Some(dlink) = file.dlink.clone().filter(|_| file.skipped.is_none()) else {
                if file.via_drive {
                    via_drive_failed += 1;
                }
                skipped.push(serde_json::json!({
                    "name": shown_name,
                    "bytes": file.size,
                    "reason": file.skipped.clone().unwrap_or_else(|| "百度没有给出下载地址".to_owned()),
                }));
                continue;
            };
            let Some(directory) = safe_relative_dir(&file.dir) else {
                skipped.push(serde_json::json!({ "name": shown_name, "bytes": file.size, "reason": "文件夹名称不安全" }));
                continue;
            };
            // Baidu's file servers accept some client identities and refuse
            // others with 403; the browser's own identity is tried first.
            let mut result = Err(String::new());
            for user_agent in [resolved.user_agent.as_str(), "pan.baidu.com", "netdisk;P2SP;3.0.0.8"] {
                let descriptor = crate::cloud_drive_core::DownloadDescriptor {
                    provider: crate::cloud_drive_core::CloudDriveProvider::Baidu,
                    url: dlink.clone(),
                    file_name: file.name.clone(),
                    headers: vec![
                        crate::cloud_drive_core::DownloadHeader { name: "User-Agent".to_owned(), value: user_agent.to_owned() },
                        crate::cloud_drive_core::DownloadHeader { name: "Referer".to_owned(), value: "https://pan.baidu.com/".to_owned() },
                        crate::cloud_drive_core::DownloadHeader { name: "Cookie".to_owned(), value: cookie_header.clone() },
                    ],
                    expected_size: Some(file.size),
                    expected_sha256: None,
                };
                result = crate::cloud_drive_core::download_file_with_client(
                    &client,
                    &descriptor,
                    &destination.join(&directory),
                    3,
                    |_| {},
                );
                if !matches!(&result, Err(error) if error.contains("403")) {
                    break;
                }
            }
            match result {
                Ok(receipt) => {
                    if file.via_drive {
                        via_drive += 1;
                    }
                    receipts.push(receipt);
                }
                Err(error) => {
                    if file.via_drive {
                        via_drive_failed += 1;
                    }
                    skipped.push(serde_json::json!({ "name": shown_name, "bytes": file.size, "reason": error }));
                }
            }
        }
        Ok::<_, String>((receipts, skipped, via_drive, via_drive_failed))
    })
    .await
    .map_err(|error| format!("Cloud Drive download task failed: {error}"))?;
    let (receipts, skipped, via_drive, via_drive_failed) = match downloaded {
        Ok(value) => value,
        Err(error) => {
            let _ = window.close();
            return Err(error);
        }
    };

    // 4. Remove the temporary drive folder once everything it held arrived.
    let mut drive_folder_deleted = false;
    if let Some(folder) = drive_folder.as_deref().filter(|_| via_drive_failed == 0) {
        while result_rx.try_recv().is_ok() {}
        let cleanup = BAIDU_CLEANUP_SCRIPT
            .replace("__FOLDER__", &serde_json::to_string(folder).expect("string"))
            .replace("__RESULT__", BAIDU_RESULT_PATH);
        if window.eval(&cleanup).is_ok() {
            if let Ok(Some(answer)) =
                tokio::time::timeout(std::time::Duration::from_secs(60), result_rx.recv()).await
            {
                drive_folder_deleted = serde_json::from_str::<Value>(&answer)
                    .ok()
                    .and_then(|value| value.get("deleted").and_then(Value::as_bool))
                    .unwrap_or(false);
            }
        }
    }
    let _ = window.close();
    Ok(serde_json::json!({
        "receipts": receipts,
        "skipped": skipped,
        "viaDrive": via_drive,
        "driveFolder": drive_folder,
        "driveFolderDeleted": drive_folder_deleted,
    }))
}

/// Deletes only the temporary folder AI-OS created in the user's drive
/// (Baidu moves it to the recycle bin). Placeholders: __FOLDER__, __RESULT__.
const BAIDU_CLEANUP_SCRIPT: &str = r#"(async () => {
  const report = (payload) => {
    location.href = "https://pan.baidu.com__RESULT__?d=" + encodeURIComponent(JSON.stringify(payload));
  };
  try {
    const folder = __FOLDER__;
    if (!folder.startsWith("/AI-OS 临时下载/")) return report({ deleted: false });
    const q = (extra) => new URLSearchParams({ channel: "chunlei", web: "1", app_id: "250528", clienttype: "0", ...extra }).toString();
    const vars = await (await fetch(`/api/gettemplatevariable?${q({ fields: JSON.stringify(["bdstoken"]) })}`, { credentials: "include" })).json();
    const token = vars && vars.result && vars.result.bdstoken;
    const result = await (await fetch(`/api/filemanager?${q({ opera: "delete", async: "2", onnest: "fail", bdstoken: token || "" })}`, {
      method: "POST", credentials: "include",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({ filelist: JSON.stringify([folder]) }),
    })).json();
    report({ deleted: result.errno === 0 });
  } catch (error) {
    report({ deleted: false });
  }
})();"#;

fn default_download_directory(share_id: &str) -> Result<String, String> {
    let downloads = dirs::download_dir()
        .or_else(|| dirs::home_dir().map(|home| home.join("Downloads")))
        .ok_or_else(|| "Unable to locate the Downloads folder.".to_owned())?;
    let folder: String = share_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    let folder = if folder.is_empty() { "share".to_owned() } else { folder };
    Ok(downloads.join("AI-OS").join(folder).to_string_lossy().into_owned())
}

/// Owner decision 2026-10-17: cloud-drive download is a built-in AI-OS
/// capability. The MCP executable ships inside AI OS.app next to the main
/// binary; nothing is downloaded, installed or published separately.
fn cloud_drive_mcp_command() -> Result<String, String> {
    let bundled = std::env::current_exe()
        .map_err(|error| format!("Unable to locate AI-OS: {error}"))?
        .with_file_name(EXECUTABLE_NAME);
    if bundled.is_file() {
        return Ok(bundled.to_string_lossy().into_owned());
    }
    Err("The built-in Cloud Drive component is missing from this AI-OS build.".to_owned())
}

/// Official bdpan installer (Baidu Netdisk, Apache-2.0,
/// github.com/baidu-netdisk/bdpan-storage, skills/baidu-drive/scripts/install.sh).
/// The version and per-platform SHA-256 are copied from that script; the
/// installer then installs the current bdpan into ~/.local/bin.
const BDPAN_INSTALLER_VERSION: &str = "3.8.4";
const BDPAN_INSTALLER_BASE: &str = "https://issuecdn.baidupcs.com/issue/netdisk/ai-bdpan/installer";

fn bdpan_installer_for_this_mac() -> Option<(&'static str, &'static str)> {
    match std::env::consts::ARCH {
        "aarch64" => Some((
            "bdpan-installer-darwin-arm64",
            "a0c395a83f9abc8f1423c30b21dfae73819376f7b1822d3bd4d3de62392c4c0c",
        )),
        "x86_64" => Some((
            "bdpan-installer-darwin-amd64",
            "cc6b10d4afea9baad77c68dadea6b9e4ecd7f8815cf364ebe6be0e51648e4623",
        )),
        _ => None,
    }
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BdpanStatus {
    installed: bool,
    signed_in: bool,
    version: Option<String>,
    token_expires_at: Option<String>,
}

fn bdpan_status_blocking() -> BdpanStatus {
    let Some(bin) = bdpan_executable() else {
        return BdpanStatus { installed: false, signed_in: false, version: None, token_expires_at: None };
    };
    let version = std::process::Command::new(&bin)
        .args(["version", "--no-check-update"])
        .output()
        .ok()
        .and_then(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .map(|line| line.trim_start_matches("bdpan:").trim().to_owned())
        });
    let whoami = bdpan_json(&bin, &["whoami"]).ok();
    let info = whoami
        .as_ref()
        .map(|value| value.get("data").filter(|data| data.is_object()).unwrap_or(value));
    let signed_in = info.is_some_and(|info| {
        info.get("authenticated").and_then(Value::as_bool).unwrap_or(false)
            && info.get("has_valid_token").and_then(Value::as_bool).unwrap_or(false)
    });
    let token_expires_at = info
        .and_then(|info| info.get("expires_at"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    BdpanStatus { installed: true, signed_in, version, token_expires_at }
}

/// Whether Baidu's official CLI is installed and signed in.
#[tauri::command]
pub async fn cloud_drive_bdpan_status() -> Result<BdpanStatus, String> {
    tauri::async_runtime::spawn_blocking(bdpan_status_blocking)
        .await
        .map_err(|error| format!("Unable to check bdpan: {error}"))
}

/// Installs bdpan with Baidu's official installer, after the user agreed.
/// The download must match the SHA-256 published in Baidu's install script.
#[tauri::command]
pub async fn cloud_drive_install_bdpan() -> Result<BdpanStatus, String> {
    tauri::async_runtime::spawn_blocking(|| -> Result<BdpanStatus, String> {
        let (name, expected) = bdpan_installer_for_this_mac()
            .ok_or_else(|| "这台电脑的处理器架构没有官方 bdpan 安装包。".to_owned())?;
        let url = format!("{BDPAN_INSTALLER_BASE}/{BDPAN_INSTALLER_VERSION}/{name}");
        let staging = std::env::temp_dir().join(format!("ai-os-bdpan-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&staging).map_err(|error| format!("无法准备安装目录：{error}"))?;
        let installer = staging.join(name);
        let result = (|| {
            let bytes = Client::builder()
                .connect_timeout(std::time::Duration::from_secs(20))
                .timeout(std::time::Duration::from_secs(300))
                .build()
                .and_then(|client| client.get(&url).send())
                .and_then(|response| response.error_for_status())
                .and_then(|response| response.bytes())
                .map_err(|error| format!("下载百度官方安装器失败：{error}"))?;
            {
                use sha2::{Digest, Sha256};
                let actual = format!("{:x}", Sha256::digest(&bytes));
                if actual != expected {
                    return Err("百度官方安装器的 SHA-256 校验没有通过，已停止安装。".to_owned());
                }
            }
            fs::write(&installer, &bytes).map_err(|error| format!("无法保存安装器：{error}"))?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&installer, fs::Permissions::from_mode(0o700))
                    .map_err(|error| format!("无法设置安装器权限：{error}"))?;
            }
            let output = std::process::Command::new(&installer)
                .arg("--yes")
                .current_dir(&staging)
                .output()
                .map_err(|error| format!("无法运行百度官方安装器：{error}"))?;
            if !output.status.success() {
                let detail: String = String::from_utf8_lossy(&output.stderr).chars().take(300).collect();
                return Err(format!("百度官方安装器运行失败：{detail}"));
            }
            Ok(())
        })();
        let _ = fs::remove_dir_all(&staging);
        result?;
        let status = bdpan_status_blocking();
        if !status.installed {
            return Err("安装器已运行，但没有找到 bdpan。".to_owned());
        }
        crate::logs::append_ai_os_diagnostic(&format!(
            "[CLOUD_DRIVE] bdpan installed {}",
            status.version.clone().unwrap_or_default()
        ));
        Ok(status)
    })
    .await
    .map_err(|error| format!("Unable to install bdpan: {error}"))?
}

const BDPAN_OAUTH_WINDOW_LABEL: &str = "cloud-drive-bdpan-authorization";
const BDPAN_CODE_PATH: &str = "/__aios_bdpan_code__";

/// Reads the authorization code from Baidu's out-of-band success page and
/// hands it back through an intercepted navigation (codes are 32 hex chars).
const BDPAN_CODE_SCRIPT: &str = r#"(() => {
  const candidates = [
    ...Array.from(document.querySelectorAll("input, textarea")).map((field) => field.value || ""),
    document.body ? document.body.innerText : "",
  ];
  for (const text of candidates) {
    const match = /\b([0-9a-f]{32})\b/i.exec(text || "");
    if (match) {
      location.href = "https://openapi.baidu.com__CODE_PATH__?c=" + encodeURIComponent(match[1]);
      return;
    }
  }
})();"#;

/// Signs bdpan in through Baidu's OAuth page, after the user agreed to the
/// bdpan safety notice. The page opens in AI-OS's Baidu store, so a user
/// already signed in to Baidu only clicks "授权"; the code is captured
/// automatically and passed to bdpan on stdin.
#[tauri::command]
pub async fn cloud_drive_bdpan_login(app: tauri::AppHandle) -> Result<BdpanStatus, String> {
    let bin = bdpan_executable().ok_or_else(|| "还没有安装 bdpan。".to_owned())?;
    let auth_bin = bin.clone();
    let auth_url = tauri::async_runtime::spawn_blocking(move || {
        std::process::Command::new(&auth_bin)
            .args(["login", "--get-auth-url", "--accept-disclaimer", "--no-check-update"])
            .output()
    })
    .await
    .map_err(|error| format!("Unable to start bdpan login: {error}"))?
    .map_err(|error| format!("无法获取百度授权链接：{error}"))?;
    let auth_url = String::from_utf8_lossy(&auth_url.stdout)
        .split_whitespace()
        .rev()
        .find(|token| token.starts_with("https://openapi.baidu.com/"))
        .map(str::to_owned)
        .ok_or_else(|| "bdpan 没有返回百度授权链接。".to_owned())?;
    let page = Url::parse(&auth_url).map_err(|_| "百度授权链接无效。".to_owned())?;

    if let Some(existing) = app.get_webview_window(BDPAN_OAUTH_WINDOW_LABEL) {
        let _ = existing.close();
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }
    let (code_tx, mut code_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let navigation_tx = code_tx.clone();
    let script = BDPAN_CODE_SCRIPT.replace("__CODE_PATH__", BDPAN_CODE_PATH);
    let window = WebviewWindowBuilder::new(&app, BDPAN_OAUTH_WINDOW_LABEL, WebviewUrl::External(page))
        .title("授权 AI-OS 使用百度网盘 — 点「授权」即可")
        .inner_size(720.0, 640.0)
        .data_store_identifier(BAIDU_DATA_STORE)
        .on_navigation(move |target| {
            let on_baidu = target.scheme() == "https"
                && target.host_str().is_some_and(|host| host == "baidu.com" || host.ends_with(".baidu.com"));
            if target.host_str() == Some("openapi.baidu.com") {
                let code = if target.path() == BDPAN_CODE_PATH {
                    target.query_pairs().find(|(key, _)| key == "c").map(|(_, value)| value.into_owned())
                } else {
                    target.query_pairs().find(|(key, _)| key == "code").map(|(_, value)| value.into_owned())
                };
                if let Some(code) = code.filter(|code| !code.is_empty()) {
                    let _ = navigation_tx.send(code);
                    return target.path() != BDPAN_CODE_PATH && on_baidu;
                }
            }
            on_baidu
        })
        .on_page_load(move |window, payload| {
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished)
                && payload.url().host_str() == Some("openapi.baidu.com")
            {
                let _ = window.eval(&script);
            }
        })
        .build()
        .map_err(|error| format!("无法打开百度授权窗口：{error}"))?;
    let _ = window.set_focus();
    drop(code_tx);

    let code = tokio::time::timeout(std::time::Duration::from_secs(600), async {
        loop {
            tokio::select! {
                code = code_rx.recv() => return code,
                _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {
                    if app.get_webview_window(BDPAN_OAUTH_WINDOW_LABEL).is_none() {
                        return None;
                    }
                }
            }
        }
    })
    .await
    .ok()
    .flatten();
    let _ = window.close();
    let code = code.ok_or_else(|| "百度授权没有完成（窗口已关闭或超时）。".to_owned())?;

    tauri::async_runtime::spawn_blocking(move || -> Result<BdpanStatus, String> {
        use std::io::Write;
        let mut child = std::process::Command::new(&bin)
            .args(["login", "--set-code-stdin", "--accept-disclaimer", "--no-check-update"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|error| format!("无法完成 bdpan 登录：{error}"))?;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(format!("{code}\n").as_bytes());
        }
        let output = child.wait_with_output().map_err(|error| format!("无法完成 bdpan 登录：{error}"))?;
        let status = bdpan_status_blocking();
        if !status.signed_in {
            let detail: String = String::from_utf8_lossy(&output.stderr).chars().take(200).collect();
            return Err(format!("bdpan 登录没有成功。{detail}"));
        }
        crate::logs::append_ai_os_diagnostic("[CLOUD_DRIVE] bdpan signed in");
        Ok(status)
    })
    .await
    .map_err(|error| format!("Unable to finish bdpan login: {error}"))?
}

/// Keeps the bdpan sign-in alive: bdpan renews its 30-day access token from
/// its long-lived refresh token when it talks to Baidu, so AI-OS makes one
/// light request shortly after launch and then once a day.
pub fn spawn_bdpan_keepalive() {
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(60));
        loop {
            if let Some(bin) = bdpan_executable() {
                if bdpan_signed_in(&bin) {
                    let _ = bdpan_json(&bin, &["ls", "/"]);
                    let expires = bdpan_status_blocking().token_expires_at.unwrap_or_default();
                    crate::logs::append_ai_os_diagnostic(&format!(
                        "[CLOUD_DRIVE] bdpan keepalive; token expires {expires}"
                    ));
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(24 * 60 * 60));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baidu_sign_in_is_seen_on_parent_domain_cookies() {
        // Owner acceptance 2026-10-17: signed in, but BDUSS on .baidu.com was
        // filtered out by an exact-host match, so the download never started.
        assert!(is_baidu_cookie_domain(".baidu.com"));
        assert!(is_baidu_cookie_domain("pan.baidu.com"));
        assert!(!is_baidu_cookie_domain("baidu.com.evil.example"));
        assert!(!is_baidu_cookie_domain("notbaidu.com"));

        let pair = |n: &str, v: &str| (n.to_owned(), v.to_owned());
        assert_eq!(baidu_authorization_header(&[pair("BAIDUID", "x")]), None);
        assert_eq!(
            baidu_authorization_header(&[
                pair("BDUSS", "u"),
                pair("STOKEN", "s"),
                pair("BDUSS", "dup"),
                pair("UNRELATED", "z"),
            ]),
            Some("BDUSS=u; STOKEN=s".to_owned())
        );
    }

}
