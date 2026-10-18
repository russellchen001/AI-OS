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
use tauri::{Emitter, Manager, WebviewUrl, WebviewWindowBuilder};
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

/// True when `<destination>/<dir>/<name>` is already there with `size` bytes
/// (that file is skipped, so it does not count toward progress).
pub(crate) fn already_downloaded(destination: &Path, dir: &str, name: &str, size: u64) -> bool {
    safe_relative_dir(dir).is_some_and(|relative| {
        destination
            .join(relative)
            .join(name)
            .metadata()
            .is_ok_and(|metadata| metadata.is_file() && metadata.len() == size)
    })
}

/// The app the cloud-drive progress events go to (set by each download).
static PROGRESS_APP: std::sync::OnceLock<tauri::AppHandle> = std::sync::OnceLock::new();

/// A cloud-drive diagnostic line; also shown in the chat as progress when it
/// names a stage the user cares about.
fn drive_log(line: &str) {
    crate::logs::append_ai_os_diagnostic(line);
    let Some(app) = PROGRESS_APP.get() else { return };
    if let Some((files, bytes)) = plan_totals(line) {
        let _ = app.emit("cloud-drive-progress", serde_json::json!({ "fileCount": files, "totalBytes": bytes }));
    } else if let Some(text) = stage_text(line) {
        let _ = app.emit("cloud-drive-progress", serde_json::json!({ "stage": text }));
    }
}

/// "[CLOUD_DRIVE] <provider> plan <files> files <bytes> bytes" → totals.
fn plan_totals(line: &str) -> Option<(u64, u64)> {
    let rest = line.strip_prefix("[CLOUD_DRIVE] ")?.split_once(" plan ")?.1;
    let mut words = rest.split_whitespace();
    let files = words.next()?.parse().ok()?;
    (words.next()? == "files").then_some(())?;
    let bytes = words.next()?.parse().ok()?;
    Some((files, bytes))
}

/// Chat wording for a "[CLOUD_DRIVE] <provider> <stage>" line.
fn stage_text(line: &str) -> Option<String> {
    let rest = line.strip_prefix("[CLOUD_DRIVE] ")?;
    let (_, stage) = rest.split_once(' ')?;
    if let Some(tail) = stage.strip_prefix("download ").and_then(|tail| tail.strip_suffix(" bytes")) {
        // "i/N size" or the older "size".
        let (position, size) = tail.split_once(' ').unwrap_or(("", tail));
        let size = size.parse::<u64>().ok()?;
        return Some(match position.split_once('/') {
            Some((index, count)) => format!("正在下载第 {index}/{count} 个文件（{}）", human_bytes(size)),
            None => format!("正在下载一个 {} 的文件", human_bytes(size)),
        });
    }
    let text = match stage {
        "open share" => "正在读取分享内容",
        "create temporary folder" => "正在你的网盘里新建临时文件夹",
        "save to drive" => "正在转存到你的网盘",
        "list saved files" => "正在读取转存的文件",
        "get download links" => "正在获取下载地址",
        "delete temporary folder" => "正在删除临时文件夹",
        "guest refused; using sign-in" => "游客不能下载这个文件，需要用你的账号",
        "via bdpan" => "正在用百度网盘官方工具转存并下载",
        "waiting for sign-in" => "请在弹出的窗口里登录（只需要登录一次）",
        "signed in" => "已登录",
        "signed in; pressing the page's 转存到云盘" => "正在转存到你的迅雷云盘",
        _ if stage.starts_with("round ") => "正在读取迅雷分享",
        _ if stage.starts_with("saved; handing") => "已交给迅雷客户端，请在迅雷里点“取回”",
        _ if stage.starts_with("waiting for Thunder") => "正在等迅雷客户端下载完成（请在迅雷里点“取回”）",
        _ => return None,
    };
    Some(text.to_owned())
}

fn human_bytes(bytes: u64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < units.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 { format!("{bytes} B") } else { format!("{value:.1} {}", units[unit]) }
}

/// Reports, every 2 s while alive, how many bytes have arrived under the
/// download folder since it started (partial files included).
struct ProgressWatch {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl ProgressWatch {
    fn start(app: &tauri::AppHandle, folder: PathBuf) -> Self {
        let _ = PROGRESS_APP.set(app.clone());
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let running = stop.clone();
        let app = app.clone();
        std::thread::spawn(move || {
            fn total(path: &Path) -> u64 {
                let Ok(entries) = fs::read_dir(path) else { return 0 };
                entries
                    .filter_map(Result::ok)
                    .map(|entry| match entry.file_type() {
                        Ok(kind) if kind.is_dir() => total(&entry.path()),
                        Ok(kind) if kind.is_file() => entry.metadata().map(|metadata| metadata.len()).unwrap_or(0),
                        _ => 0,
                    })
                    .sum()
            }
            let baseline = total(&folder);
            let mut last = 0;
            while !running.load(std::sync::atomic::Ordering::Relaxed) {
                let bytes = total(&folder).saturating_sub(baseline);
                if bytes != last {
                    last = bytes;
                    let _ = app.emit("cloud-drive-progress", serde_json::json!({ "bytes": bytes }));
                }
                std::thread::sleep(std::time::Duration::from_secs(2));
            }
        });
        Self { stop }
    }
}

impl Drop for ProgressWatch {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
    }
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
pub(crate) fn safe_relative_dir(dir: &str) -> Option<PathBuf> {
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

pub(crate) fn sha256_hex(path: &Path) -> Result<String, String> {
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
    let todo: Vec<u64> = expected
        .iter()
        .filter(|(relative, size)| {
            let (dir, name) = relative.rsplit_once('/').unwrap_or(("", relative.as_str()));
            !already_downloaded(&destination, dir, name, *size)
        })
        .map(|(_, size)| *size)
        .collect();
    drive_log(&format!("[CLOUD_DRIVE] baidu plan {} files {} bytes", todo.len(), todo.iter().sum::<u64>()));
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
    let _progress = ProgressWatch::start(&app, PathBuf::from(default_download_directory(&share.share_id)?));
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
            drive_log("[CLOUD_DRIVE] baidu via bdpan");
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
                    drive_log(&format!("[CLOUD_DRIVE] baidu {stage}"));
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
    drive_log("[CLOUD_DRIVE] baidu signed in; resolving share off-screen");

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

/// A cloud drive whose web sign-in AI-OS keeps in its own persistent store.
struct WebSignIn {
    provider_name: &'static str,
    window_label: &'static str,
    data_store: [u8; 16],
    login_url: &'static str,
    /// Cookie domains (and their subdomains) that belong to this drive.
    domains: &'static [&'static str],
    /// For drives that keep the session in page storage instead of cookies:
    /// a script that navigates to `https://<host>__aios_credential__?d=…`
    /// with the credential (the navigation is intercepted, never loaded).
    credential_script: Option<&'static str>,
}

const CREDENTIAL_PATH: &str = "/__aios_credential__";

fn in_domains(domain: &str, domains: &[&str]) -> bool {
    let domain = domain.trim_start_matches('.').to_ascii_lowercase();
    domains.iter().any(|root| domain == *root || domain.ends_with(&format!(".{root}")))
}

/// The drive's cookies in AI-OS's store, one per name (the most specific
/// domain wins), as a request header.
fn web_sign_in_cookie_header(window: &tauri::WebviewWindow, domains: &[&str]) -> Result<String, String> {
    let mut chosen: Vec<(String, String, usize)> = Vec::new();
    for cookie in window.cookies().map_err(|error| format!("无法读取登录状态：{error}"))? {
        let Some(domain) = cookie.domain() else { continue };
        if !in_domains(domain, domains) || cookie.value().is_empty() {
            continue;
        }
        let specificity = domain.trim_start_matches('.').len();
        match chosen.iter_mut().find(|(name, _, _)| name == cookie.name()) {
            Some(existing) if specificity >= existing.2 => {
                existing.1 = cookie.value().to_owned();
                existing.2 = specificity;
            }
            Some(_) => {}
            None => chosen.push((cookie.name().to_owned(), cookie.value().to_owned(), specificity)),
        }
    }
    Ok(chosen.into_iter().map(|(name, value, _)| format!("{name}={value}")).collect::<Vec<_>>().join("; "))
}

/// Owner request 2026-10-18: sign in once, then keep working. Opens the
/// drive's own sign-in page in AI-OS's persistent store; when the saved
/// session already works the window never appears. Returns the session
/// credential — the cookie header, or what `credential_script` reports —
/// in memory only, never logged.
async fn web_sign_in(
    app: &tauri::AppHandle,
    drive: &WebSignIn,
    signed_in: impl Fn(String) -> bool + Send + Sync + Clone + 'static,
) -> Result<String, String> {
    if let Some(existing) = app.get_webview_window(drive.window_label) {
        let _ = existing.close();
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }
    let page = Url::parse(drive.login_url).map_err(|_| "登录地址无效".to_owned())?;
    let reported = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let reporter = reported.clone();
    let window = WebviewWindowBuilder::new(app, drive.window_label, WebviewUrl::External(page))
        .title(format!("登录{} — 只需登录一次，AI-OS 会自动下载", drive.provider_name))
        .inner_size(960.0, 720.0)
        .visible(false)
        .data_store_identifier(drive.data_store)
        .on_navigation(move |target| {
            if target.path() == CREDENTIAL_PATH {
                if let Some((_, value)) = target.query_pairs().find(|(key, _)| key == "d") {
                    if let Ok(mut slot) = reporter.lock() {
                        *slot = value.into_owned();
                    }
                }
                return false;
            }
            target.scheme() == "https"
        })
        .build()
        .map_err(|error| format!("无法打开{}登录窗口：{error}", drive.provider_name))?;
    let started = std::time::Instant::now();
    let mut shown = false;
    let mut last_checked = String::new();
    loop {
        if app.get_webview_window(drive.window_label).is_none() {
            return Err(format!("{}登录窗口被关闭，下载已取消。", drive.provider_name));
        }
        let cookie = match drive.credential_script {
            Some(script) => {
                let _ = window.eval(script);
                reported.lock().map(|value| value.clone()).unwrap_or_default()
            }
            None => web_sign_in_cookie_header(&window, drive.domains)?,
        };
        // Only ask the drive again when the cookies changed.
        if !cookie.is_empty() && cookie != last_checked {
            last_checked = cookie.clone();
            let check = signed_in.clone();
            let probe = cookie.clone();
            if tauri::async_runtime::spawn_blocking(move || check(probe)).await.unwrap_or(false) {
                let _ = window.close();
                drive_log(&format!("[CLOUD_DRIVE] {} signed in", drive.window_label));
                return Ok(cookie);
            }
        }
        // Longer than Baidu's grace: the saved session is confirmed with
        // the drive (and Aliyun's page may first renew its token).
        if !shown && started.elapsed() >= std::time::Duration::from_secs(6) {
            let _ = window.show();
            let _ = window.set_focus();
            shown = true;
            drive_log(&format!("[CLOUD_DRIVE] {} waiting for sign-in", drive.window_label));
        }
        if started.elapsed() >= BAIDU_LOGIN_TIMEOUT {
            let _ = window.close();
            return Err(format!("等了 10 分钟仍未完成{}登录，下载已取消。", drive.provider_name));
        }
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
    }
}

const QUARK_SIGN_IN: WebSignIn = WebSignIn {
    provider_name: "夸克网盘",
    window_label: "cloud-drive-quark-authorization",
    data_store: *b"AI-OS-quark-auth",
    login_url: "https://pan.quark.cn/",
    domains: &["quark.cn"],
    credential_script: None,
};

/// Aliyun's web client keeps its token in localStorage ("token"); the page
/// renews it by itself, so AI-OS only reads a current one.
const ALIYUN_SIGN_IN: WebSignIn = WebSignIn {
    provider_name: "阿里云盘",
    window_label: "cloud-drive-aliyun-authorization",
    data_store: *b"AI-OS-alipanauth",
    login_url: "https://www.alipan.com/drive",
    domains: &["alipan.com", "aliyundrive.com"],
    credential_script: Some(
        r#"(() => { try {
  const token = JSON.parse(localStorage.getItem("token") || "null");
  if (token && token.access_token && !(token.expire_time && Date.parse(token.expire_time) < Date.now() + 60000)) {
    location.href = location.origin + "/__aios_credential__?d=" + encodeURIComponent(token.access_token);
  }
} catch (error) {} })();"#,
    ),
};

/// Aliyun share → files on disk: anonymous share reading, then the user's
/// own sign-in (asked for once, kept in AI-OS) for the download URLs.
#[tauri::command]
pub async fn download_aliyun_share_automatically(
    app: tauri::AppHandle,
    url: String,
    extraction_code: String,
) -> Result<Value, String> {
    let share = ShareLink::parse(&url, Some(&extraction_code))?;
    let _progress = ProgressWatch::start(&app, PathBuf::from(default_download_directory(&share.share_id)?));
    if share.provider != crate::cloud_drive_core::CloudDriveProvider::Aliyun {
        return Err("这不是阿里云盘的分享链接。".to_owned());
    }
    let share_id = share.share_id.clone();
    let share_pwd = extraction_code.trim().to_owned();
    {
        let (share_id, share_pwd) = (share_id.clone(), share_pwd.clone());
        tauri::async_runtime::spawn_blocking(move || {
            crate::cloud_drive_aliyun::AliyunSession::new("")?.share_token(&share_id, &share_pwd).map(|_| ())
        })
        .await
        .map_err(|error| format!("Cloud Drive download task failed: {error}"))??;
    }
    let token = web_sign_in(&app, &ALIYUN_SIGN_IN, |token| {
        crate::cloud_drive_aliyun::AliyunSession::new(&token).is_ok_and(|session| session.signed_in())
    })
    .await?;
    let destination = PathBuf::from(default_download_directory(&share.share_id)?);
    tauri::async_runtime::spawn_blocking(move || {
        crate::cloud_drive_aliyun::download_share(&token, &share_id, &share_pwd, &destination, |stage| {
            drive_log(&format!("[CLOUD_DRIVE] aliyun {stage}"));
        })
    })
    .await
    .map_err(|error| format!("Cloud Drive download task failed: {error}"))?
}

const UC_SIGN_IN: WebSignIn = WebSignIn {
    provider_name: "UC 网盘",
    window_label: "cloud-drive-uc-authorization",
    data_store: *b"AI-OS-ucpan-auth",
    login_url: "https://drive.uc.cn/",
    domains: &["uc.cn"],
    credential_script: None,
};

/// Quark share → files on disk: anonymous share reading, the user's own
/// session (signed in once in AI-OS) to save, download and clean up.
#[tauri::command]
pub async fn download_quark_share_automatically(
    app: tauri::AppHandle,
    url: String,
    extraction_code: String,
) -> Result<Value, String> {
    download_quark_service_share(app, url, extraction_code, &crate::cloud_drive_quark::QUARK, &QUARK_SIGN_IN).await
}

/// UC share → files on disk, exactly as Quark (same service).
#[tauri::command]
pub async fn download_uc_share_automatically(
    app: tauri::AppHandle,
    url: String,
    extraction_code: String,
) -> Result<Value, String> {
    download_quark_service_share(app, url, extraction_code, &crate::cloud_drive_quark::UC, &UC_SIGN_IN).await
}

async fn download_quark_service_share(
    app: tauri::AppHandle,
    url: String,
    extraction_code: String,
    flavor: &'static crate::cloud_drive_quark::Flavor,
    sign_in: &'static WebSignIn,
) -> Result<Value, String> {
    let share = ShareLink::parse(&url, Some(&extraction_code))?;
    let _progress = ProgressWatch::start(&app, PathBuf::from(default_download_directory(&share.share_id)?));
    if share.provider != flavor.provider {
        return Err(format!("这不是{}的分享链接。", sign_in.provider_name));
    }
    // Check the share before asking anyone to sign in.
    let pwd_id = share.share_id.clone();
    let passcode = extraction_code.trim().to_owned();
    {
        let (pwd_id, passcode) = (pwd_id.clone(), passcode.clone());
        tauri::async_runtime::spawn_blocking(move || {
            crate::cloud_drive_quark::QuarkSession::new(flavor, "")?.share_token(&pwd_id, &passcode).map(|_| ())
        })
        .await
        .map_err(|error| format!("Cloud Drive download task failed: {error}"))??;
    }
    let cookie = web_sign_in(&app, sign_in, move |cookie| {
        crate::cloud_drive_quark::QuarkSession::new(flavor, &cookie).is_ok_and(|session| session.signed_in())
    })
    .await?;
    let destination = PathBuf::from(default_download_directory(&share.share_id)?);
    tauri::async_runtime::spawn_blocking(move || {
        crate::cloud_drive_quark::download_share(flavor, &cookie, &pwd_id, &passcode, &destination, |stage| {
            drive_log(&format!("[CLOUD_DRIVE] {} {stage}", flavor.key));
        })
    })
    .await
    .map_err(|error| format!("Cloud Drive download task failed: {error}"))?
}

/// 123's web client keeps its token in localStorage ("authorToken").
const PAN123_SIGN_IN: WebSignIn = WebSignIn {
    provider_name: "123 云盘",
    window_label: "cloud-drive-123-authorization",
    data_store: *b"AI-OS-123pan-aut",
    login_url: "https://www.123pan.cn/",
    domains: &["123pan.cn", "123pan.com"],
    credential_script: Some(
        r#"(() => { try {
  const token = localStorage.getItem("authorToken");
  if (token && token.replace(/"/g, "").length > 20) {
    location.href = location.origin + "/__aios_credential__?d=" + encodeURIComponent(token.replace(/"/g, ""));
  }
} catch (error) {} })();"#,
    ),
};

/// 123 share → files on disk. Guests may download 123 shares, so no sign-in
/// is asked for unless 123 refuses a file to guests.
#[tauri::command]
pub async fn download_123_share_automatically(
    app: tauri::AppHandle,
    url: String,
    extraction_code: String,
) -> Result<Value, String> {
    let share = ShareLink::parse(&url, Some(&extraction_code))?;
    let _progress = ProgressWatch::start(&app, PathBuf::from(default_download_directory(&share.share_id)?));
    if share.provider != crate::cloud_drive_core::CloudDriveProvider::Pan123 {
        return Err("这不是 123 云盘的分享链接。".to_owned());
    }
    let share_key = share.share_id.clone();
    let share_pwd = extraction_code.trim().to_owned();
    let destination = PathBuf::from(default_download_directory(&share_key)?);
    tauri::async_runtime::spawn_blocking(move || {
        crate::cloud_drive_123::download_share(
            &share_key,
            &share_pwd,
            &destination,
            || {
                tauri::async_runtime::block_on(web_sign_in(&app, &PAN123_SIGN_IN, |token| {
                    crate::cloud_drive_123::Pan123Session::new(&token).is_ok_and(|session| session.signed_in())
                }))
            },
            |stage| drive_log(&format!("[CLOUD_DRIVE] pan123 {stage}")),
        )
    })
    .await
    .map_err(|error| format!("Cloud Drive download task failed: {error}"))?
}

const CLOUD189_SIGN_IN: WebSignIn = WebSignIn {
    provider_name: "天翼云盘",
    window_label: "cloud-drive-189-authorization",
    data_store: *b"AI-OS-cloud189-a",
    login_url: "https://cloud.189.cn/web/main/",
    domains: &["189.cn"],
    credential_script: None,
};

/// 天翼云盘 share → files on disk: anonymous share reading (with the access
/// code), then the user's own sign-in (asked for once) for download URLs.
#[tauri::command]
pub async fn download_189_share_automatically(
    app: tauri::AppHandle,
    url: String,
    extraction_code: String,
) -> Result<Value, String> {
    let share = ShareLink::parse(&url, Some(&extraction_code))?;
    let _progress = ProgressWatch::start(&app, PathBuf::from(default_download_directory(&share.share_id)?));
    if share.provider != crate::cloud_drive_core::CloudDriveProvider::ChinaTelecom {
        return Err("这不是天翼云盘的分享链接。".to_owned());
    }
    let share_code = share.share_id.clone();
    let access_code = extraction_code.trim().to_owned();
    {
        let (share_code, access_code) = (share_code.clone(), access_code.clone());
        tauri::async_runtime::spawn_blocking(move || {
            crate::cloud_drive_189::Cloud189Session::new("")?.open_share(&share_code, &access_code).map(|_| ())
        })
        .await
        .map_err(|error| format!("Cloud Drive download task failed: {error}"))??;
    }
    let cookie = web_sign_in(&app, &CLOUD189_SIGN_IN, |cookie| {
        crate::cloud_drive_189::Cloud189Session::new(&cookie).is_ok_and(|session| session.signed_in())
    })
    .await?;
    let destination = PathBuf::from(default_download_directory(&share.share_id)?);
    tauri::async_runtime::spawn_blocking(move || {
        crate::cloud_drive_189::download_share(&cookie, &share_code, &access_code, &destination, |stage| {
            drive_log(&format!("[CLOUD_DRIVE] 189 {stage}"));
        })
    })
    .await
    .map_err(|error| format!("Cloud Drive download task failed: {error}"))?
}

const CAIYUN_SIGN_IN: WebSignIn = WebSignIn {
    provider_name: "移动云盘",
    window_label: "cloud-drive-139-authorization",
    data_store: *b"AI-OS-caiyun-aut",
    login_url: "https://yun.139.com/",
    domains: &["139.com", "10086.cn"],
    credential_script: None,
};

/// 移动云盘 share → files on disk: anonymous (encrypted) share reading, then
/// the user's own sign-in (asked for once) for download URLs.
#[tauri::command]
pub async fn download_139_share_automatically(
    app: tauri::AppHandle,
    url: String,
    extraction_code: String,
) -> Result<Value, String> {
    let share = ShareLink::parse(&url, Some(&extraction_code))?;
    let _progress = ProgressWatch::start(&app, PathBuf::from(default_download_directory(&share.share_id)?));
    if share.provider != crate::cloud_drive_core::CloudDriveProvider::ChinaMobile {
        return Err("这不是移动云盘的分享链接。".to_owned());
    }
    let link_id = share.share_id.clone();
    let passwd = extraction_code.trim().to_owned();
    {
        let (link_id, passwd) = (link_id.clone(), passwd.clone());
        tauri::async_runtime::spawn_blocking(move || {
            crate::cloud_drive_139::CaiyunSession::new(None)?.list(&link_id, &passwd, "root").map(|_| ())
        })
        .await
        .map_err(|error| format!("Cloud Drive download task failed: {error}"))??;
    }
    let cookie = web_sign_in(&app, &CAIYUN_SIGN_IN, |cookie| {
        crate::cloud_drive_139::credential_from_cookies(&cookie).is_some()
    })
    .await?;
    let credential = crate::cloud_drive_139::credential_from_cookies(&cookie)
        .ok_or_else(|| "没有读到移动云盘的登录凭据。".to_owned())?;
    let destination = PathBuf::from(default_download_directory(&share.share_id)?);
    tauri::async_runtime::spawn_blocking(move || {
        crate::cloud_drive_139::download_share(credential, &link_id, &passwd, &destination, |stage| {
            drive_log(&format!("[CLOUD_DRIVE] 139 {stage}"));
        })
    })
    .await
    .map_err(|error| format!("Cloud Drive download task failed: {error}"))?
}

/// Owner decision 2026-10-18: Xunlei works like Baidu's web route. Xunlei's
/// share API wants a short-lived captcha token that Xunlei's own share page
/// obtains; AI-OS never computes one. This script runs before the page's own
/// scripts and only remembers the client headers the page itself sends to
/// api-pan.xunlei.com, so AI-OS's in-page calls carry the page's own token.
const XUNLEI_CAPTURE_SCRIPT: &str = r#"(() => {
  const seen = (window.__aiosXunlei = window.__aiosXunlei || {});
  const keep = (url, name, value) => {
    if (!String(url || "").includes("api-pan.xunlei.com")) return;
    const key = String(name).toLowerCase();
    if (key === "x-captcha-token") seen.token = value;
    if (key === "x-device-id") seen.device = value;
    if (key === "x-client-id") seen.client = value;
    if (key === "x-client-version") seen.version = value;
    if (key === "authorization" && value) seen.authorization = value;
  };
  const open = XMLHttpRequest.prototype.open;
  XMLHttpRequest.prototype.open = function (method, url) { this.__aiosUrl = url; return open.apply(this, arguments); };
  const setHeader = XMLHttpRequest.prototype.setRequestHeader;
  XMLHttpRequest.prototype.setRequestHeader = function (name, value) { keep(this.__aiosUrl, name, value); return setHeader.apply(this, arguments); };
  // The page's own restore answer (share/restore, then tasks/<id> while it
  // runs) carries params.trace_file_ids: share file id → new drive file id.
  const answer = (url, text) => {
    const address = String(url || "");
    if (!address.includes("api-pan.xunlei.com") || !(/\/share\/restore(\?|$)/.test(address) || address.includes("/tasks/"))) return;
    try {
      const body = JSON.parse(text);
      if (body.error) { seen.restoreError = body.error_description || body.error; return; }
      const status = String(body.restore_status || body.phase || "").toUpperCase();
      const trace = body.params && body.params.trace_file_ids;
      if (/COMPLETE/.test(status) && trace) seen.traceIds = typeof trace === "string" ? JSON.parse(trace) : trace;
      if (/ERROR|FAIL/.test(status)) seen.restoreError = status;
    } catch (error) {}
  };
  const send = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.send = function () {
    this.addEventListener("load", () => answer(this.__aiosUrl, this.responseText));
    return send.apply(this, arguments);
  };
  const originalFetch = window.fetch;
  window.fetch = function (input, init) {
    const url = typeof input === "string" ? input : input && input.url;
    try {
      const headers = new Headers((init && init.headers) || (input && input.headers) || {});
      headers.forEach((value, name) => keep(url, name, value));
    } catch (error) {}
    const pending = originalFetch.apply(this, arguments);
    pending.then((response) => response.clone().text().then((text) => answer(url, text))).catch(() => {});
    return pending;
  };
})();"#;

/// Runs inside Xunlei's share page with the headers it captured: lists the
/// share and asks for each file's download link, exactly the calls the page
/// makes when a visitor opens folders and clicks download. Placeholders:
/// __SHARE__, __PWD__, __TOKEN__ (pass-code token from an earlier round, or
/// ""), __IDS__ (file ids still missing a link, or null for a full run),
/// __RESULT__.
const XUNLEI_SHARE_SCRIPT: &str = r#"(async () => {
  const report = (payload) => {
    location.href = "https://pan.xunlei.com__RESULT__?d=" + encodeURIComponent(JSON.stringify(payload));
  };
  const seen = window.__aiosXunlei || {};
  if (!seen.token) return report({ needToken: true });
  const shareId = __SHARE__, passCode = __PWD__, missing = __IDS__;
  let passCodeToken = __TOKEN__;
  const headers = { Accept: "application/json" };
  if (seen.token) headers["X-Captcha-Token"] = seen.token;
  if (seen.device) headers["X-Device-Id"] = seen.device;
  if (seen.client) headers["X-Client-Id"] = seen.client;
  if (seen.version) headers["X-Client-Version"] = seen.version;
  if (seen.authorization) headers["Authorization"] = seen.authorization;
  const expired = (body) => /captcha/i.test(String(body.error || "")) || body.error_code === 9;
  const get = async (path, params) => {
    const response = await fetch("https://api-pan.xunlei.com/drive/v1/" + path + "?" + new URLSearchParams(params), { headers });
    const body = await response.json();
    if (body.error) {
      const error = new Error(body.error_description || body.error);
      error.expired = expired(body);
      throw error;
    }
    return body;
  };
  const sizeOf = (item) => Number(item.size || 0);
  const linkOf = (info) => {
    if (info.web_content_link) return info.web_content_link;
    const links = info.links || {};
    for (const key of Object.keys(links)) if (links[key] && links[key].url) return links[key].url;
    return "";
  };
  const files = [];
  let tops = [];
  try {
    if (!missing) {
      const top = [];
      let page = "";
      do {
        const body = await get("share", { share_id: shareId, pass_code: passCode, pass_code_token: passCodeToken, limit: "100", page_token: page, thumbnail_size: "SIZE_SMALL" });
        if (body.share_status !== "OK") {
          const need = body.share_status === "PASS_CODE_EMPTY" || body.share_status === "PASS_CODE_ERROR";
          return report({ error: need ? "这个迅雷云盘分享需要提取码（或提取码不对），请把提取码一起发给我" : "迅雷云盘分享不可用（" + (body.share_status_text || body.share_status) + "）" });
        }
        if (body.pass_code_token) passCodeToken = body.pass_code_token;
        top.push(...(body.files || []).map((item) => ({ dir: "", item })));
        page = body.next_page_token || "";
      } while (page);
      tops = top.map((entry) => ({ id: entry.item.id, name: entry.item.name, size: String(entry.item.size || "0") }));
      const pending = top;
      while (pending.length) {
        const { dir, item } = pending.pop();
        if (item.kind === "drive#folder") {
          const path = dir ? dir + "/" + item.name : item.name;
          let folderPage = "";
          do {
            const body = await get("share/detail", { share_id: shareId, parent_id: item.id, pass_code_token: passCodeToken, limit: "100", page_token: folderPage, thumbnail_size: "SIZE_SMALL", space: "", scene: "NORMAL" });
            pending.push(...(body.files || []).map((child) => ({ dir: path, item: child })));
            folderPage = body.next_page_token || "";
          } while (folderPage);
        } else {
          files.push({ id: item.id, dir, name: item.name, size: sizeOf(item), url: "" });
        }
        if (files.length > 10000) return report({ error: "分享超过 10000 个文件" });
      }
    } else {
      files.push(...missing.map((file) => ({ ...file, url: "" })));
    }
    let diagnostic = "";
    for (const file of files) {
      const body = await get("share/file_info", { share_id: shareId, file_id: file.id, pass_code: passCode, pass_code_token: passCodeToken, folder_id: "", space: "", scene: "NORMAL" });
      const info = body.file_info || body;
      file.url = linkOf(info);
      // Field names only (never values) for the log, to see why a link is missing.
      if (!file.url && !diagnostic) {
        diagnostic = "top=" + Object.keys(body).join(",") + " info=" + Object.keys(info || {}).join(",") +
          " links=" + Object.keys((info && info.links) || {}).join(",") + " phase=" + String((info && info.phase) || "") +
          " audit=" + String((info && info.audit && info.audit.status) || "");
      }
    }
    report({ files, tops, passCodeToken, userAgent: navigator.userAgent, diagnostic });
  } catch (error) {
    if (error.expired && files.length) return report({ files, tops, passCodeToken, userAgent: navigator.userAgent, expired: true });
    report({ error: "迅雷云盘：" + (error.message || String(error)) });
  }
})();"#;

const XUNLEI_WINDOW_LABEL: &str = "cloud-drive-xunlei";
const XUNLEI_RESULT_PATH: &str = "/__aios_result__";

#[derive(Debug, Deserialize, serde::Serialize, Clone)]
struct XunleiFile {
    id: String,
    dir: String,
    name: String,
    size: u64,
    url: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct XunleiScriptResult {
    #[serde(default)]
    need_token: bool,
    #[serde(default)]
    files: Vec<XunleiFile>,
    #[serde(default)]
    pass_code_token: String,
    #[serde(default)]
    user_agent: String,
    #[serde(default)]
    expired: bool,
    #[serde(default)]
    diagnostic: String,
    #[serde(default)]
    tops: Vec<Value>,
    error: Option<String>,
}

/// Xunlei share → files on disk. Xunlei's own share page (off-screen, in
/// AI-OS's persistent store) obtains its captcha token; AI-OS lists the share
/// and gets each file's guest link inside that page, then downloads and
/// verifies natively. If Xunlei asks the visitor to verify, the window is
/// brought on screen for the user.
#[tauri::command]
pub async fn download_xunlei_share_automatically(
    app: tauri::AppHandle,
    url: String,
    extraction_code: String,
) -> Result<Value, String> {
    let share = ShareLink::parse(&url, Some(&extraction_code))?;
    let _progress = ProgressWatch::start(&app, PathBuf::from(default_download_directory(&share.share_id)?));
    if share.provider != crate::cloud_drive_core::CloudDriveProvider::Xunlei {
        return Err("这不是迅雷云盘的分享链接。".to_owned());
    }
    let pass_code = extraction_code.trim().to_owned();
    if let Some(existing) = app.get_webview_window(XUNLEI_WINDOW_LABEL) {
        let _ = existing.close();
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    }
    let mut page = Url::parse(&format!("https://pan.xunlei.com/s/{}", share.share_id))
        .map_err(|_| "迅雷云盘分享地址无效。".to_owned())?;
    if !pass_code.is_empty() {
        page.query_pairs_mut().append_pair("pwd", &pass_code);
    }
    let (result_tx, mut result_rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let (load_tx, mut load_rx) = tokio::sync::mpsc::unbounded_channel::<()>();
    let window = WebviewWindowBuilder::new(&app, XUNLEI_WINDOW_LABEL, WebviewUrl::External(page.clone()))
        .title("迅雷云盘 — 如需验证请在此完成，AI-OS 会自动下载")
        .inner_size(960.0, 720.0)
        .visible(false)
        .data_store_identifier(*b"AI-OS-xunlei-aut")
        .initialization_script(XUNLEI_CAPTURE_SCRIPT)
        .on_navigation(move |target| {
            if target.host_str() == Some("pan.xunlei.com") && target.path() == XUNLEI_RESULT_PATH {
                if let Some((_, payload)) = target.query_pairs().find(|(key, _)| key == "d") {
                    let _ = result_tx.send(payload.into_owned());
                }
                return false;
            }
            target.scheme() == "https"
        })
        .on_page_load(move |_, payload| {
            if matches!(payload.event(), tauri::webview::PageLoadEvent::Finished) {
                let _ = load_tx.send(());
            }
        })
        .build()
        .map_err(|error| format!("无法打开迅雷云盘页面：{error}"))?;
    // Off-screen but "visible": macOS suspends pages of hidden windows.
    let _ = window.set_position(tauri::PhysicalPosition::new(-32000, -32000));
    let _ = window.set_skip_taskbar(true);
    let _ = window.show();
    let close = |window: &tauri::WebviewWindow| {
        let _ = window.close();
    };

    let mut files: Vec<XunleiFile> = Vec::new();
    let mut pass_code_token = String::new();
    let mut user_agent = String::new();
    let mut missing: Option<Vec<XunleiFile>> = None;
    let mut top_items: Vec<Value> = Vec::new();
    let started = std::time::Instant::now();
    let mut shown = false;
    'rounds: for round in 0..12 {
        if round > 0 {
            // A fresh page load gives a fresh token from Xunlei's own page.
            while load_rx.try_recv().is_ok() {}
            if window.navigate(page.clone()).is_err() {
                break;
            }
        }
        // Each round waits on its own: a reload is not a reason to show the window.
        let round_started = std::time::Instant::now();
        let loaded = tokio::time::timeout(std::time::Duration::from_secs(60), load_rx.recv()).await;
        if !matches!(loaded, Ok(Some(()))) {
            close(&window);
            return Err("打开迅雷云盘分享页面超时。".to_owned());
        }
        loop {
            if app.get_webview_window(XUNLEI_WINDOW_LABEL).is_none() {
                return Err("迅雷云盘窗口被关闭，下载已取消。".to_owned());
            }
            while result_rx.try_recv().is_ok() {}
            let ids = match &missing {
                Some(list) => serde_json::to_string(list).expect("files"),
                None => "null".to_owned(),
            };
            let script = XUNLEI_SHARE_SCRIPT
                .replace("__SHARE__", &serde_json::to_string(&share.share_id).expect("string"))
                .replace("__PWD__", &serde_json::to_string(&pass_code).expect("string"))
                .replace("__TOKEN__", &serde_json::to_string(&pass_code_token).expect("string"))
                .replace("__IDS__", &ids)
                .replace("__RESULT__", XUNLEI_RESULT_PATH);
            if window.eval(&script).is_err() {
                close(&window);
                return Err("无法在迅雷云盘页面里读取分享。".to_owned());
            }
            let Ok(Some(payload)) = tokio::time::timeout(std::time::Duration::from_secs(300), result_rx.recv()).await else {
                close(&window);
                return Err("迅雷云盘 5 分钟内没有返回分享内容。".to_owned());
            };
            let result: XunleiScriptResult =
                serde_json::from_str(&payload).map_err(|_| "迅雷云盘返回的分享内容无法读取。".to_owned())?;
            if let Some(error) = result.error {
                close(&window);
                return Err(error);
            }
            if result.need_token {
                // Xunlei's page has not called its API yet; after a while it
                // may be waiting for the visitor to verify — show it.
                if !shown && round_started.elapsed() >= std::time::Duration::from_secs(45) {
                    let _ = window.center();
                    let _ = window.set_skip_taskbar(false);
                    let _ = window.set_focus();
                    shown = true;
                }
                if started.elapsed() >= BAIDU_LOGIN_TIMEOUT {
                    close(&window);
                    return Err("迅雷云盘页面 10 分钟内没有就绪，下载已取消。".to_owned());
                }
                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                continue;
            }
            if !result.pass_code_token.is_empty() {
                pass_code_token = result.pass_code_token;
            }
            if !result.tops.is_empty() {
                top_items = result.tops;
            }
            if !result.user_agent.is_empty() {
                user_agent = result.user_agent;
            }
            for file in result.files {
                match files.iter_mut().find(|known| known.id == file.id) {
                    Some(known) if !file.url.is_empty() => known.url = file.url,
                    Some(_) => {}
                    None => files.push(file),
                }
            }
            let still_missing: Vec<XunleiFile> = files.iter().filter(|file| file.url.is_empty()).cloned().collect();
            crate::logs::append_ai_os_diagnostic(&format!(
                "[CLOUD_DRIVE] xunlei round {round}: {} files, {} without link{}{}",
                files.len(),
                still_missing.len(),
                if result.expired { " (token expired)" } else { "" },
                if result.diagnostic.is_empty() { String::new() } else { format!("; {}", result.diagnostic) }
            ));
            // Owner test 2026-10-18: a missing link is Xunlei's file-type rule
            // for guests (same files every round), so only an expired token
            // is worth a fresh page load.
            if still_missing.is_empty() || !result.expired {
                break 'rounds;
            }
            missing = Some(still_missing);
            continue 'rounds;
        }
    }
    if files.is_empty() {
        close(&window);
        return Err("分享里没有可下载的文件".to_owned());
    }

    // Files Xunlei keeps from guests (by type): Xunlei's own page, signed in
    // once in this window's store, saves the share to the user's drive with
    // its own 转存 button; the files are then handed to the user's Thunder
    // app exactly as the page's "已安装，点此下载资源" does on macOS (file list in
    // the clipboard + xunleiapp://mac.thunder/downloadCloudFilesFromClipboard).
    // Xunlei's web client refuses drive downloads in a browser ("请使用迅雷客户端下载",
    // owner check 2026-10-18), so the app is the only route.
    let missing_files: Vec<XunleiFile> = files.iter().filter(|file| file.url.is_empty()).cloned().collect();
    let mut handoff_note: Option<String> = None;
    let mut handed_off = false;
    if !missing_files.is_empty() {
        match xunlei_save_and_hand_off(&app, &window, &mut result_rx, &mut load_rx, &page, &top_items, &missing_files).await {
            Ok(()) => handed_off = true,
            Err(note) => handoff_note = Some(note),
        }
    }
    close(&window);
    let user_agent = if user_agent.is_empty() { "Mozilla/5.0".to_owned() } else { user_agent };

    let destination = PathBuf::from(default_download_directory(&share.share_id)?);
    let thunder_folder = thunder_save_folder();
    tauri::async_runtime::spawn_blocking(move || {
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(20))
            .build()
            .map_err(|error| format!("无法创建下载客户端：{error}"))?;
        let mut receipts = Vec::new();
        let mut skipped = Vec::new();
        let mut pending_in_thunder: Vec<(String, PathBuf, u64)> = Vec::new();
        let todo: Vec<u64> = files
            .iter()
            .filter(|file| !file.url.is_empty() && !already_downloaded(&destination, &file.dir, &file.name, file.size))
            .map(|file| file.size)
            .collect();
        drive_log(&format!("[CLOUD_DRIVE] xunlei plan {} files {} bytes", todo.len(), todo.iter().sum::<u64>()));
        let mut index = 0;
        for file in files {
            let shown_name = if file.dir.is_empty() { file.name.clone() } else { format!("{}/{}", file.dir, file.name) };
            let skip = |reason: String| serde_json::json!({ "name": shown_name, "bytes": file.size, "reason": reason });
            let Some(directory) = safe_relative_dir(&file.dir) else {
                skipped.push(skip("文件夹名称不安全".to_owned()));
                continue;
            };
            if file.url.is_empty() {
                if handed_off {
                    pending_in_thunder.push((shown_name.clone(), thunder_folder.join(&directory).join(&file.name), file.size));
                } else {
                    skipped.push(skip(format!(
                        "迅雷只允许游客下载部分类型的文件，这个文件要用迅雷客户端下载{}",
                        handoff_note.as_deref().map(|note| format!("（{note}）")).unwrap_or_default()
                    )));
                }
                continue;
            }
            let directory = destination.join(directory);
            let target = directory.join(&file.name);
            if target.metadata().is_ok_and(|metadata| metadata.is_file() && metadata.len() == file.size) {
                receipts.push(serde_json::json!({
                    "provider": "xunlei", "path": target.to_string_lossy(), "bytes": file.size,
                    "sha256": sha256_hex(&target)?, "attempts": 0, "integrityVerified": true,
                }));
                continue;
            }
            let url = if file.url.starts_with("http://") { file.url.replacen("http://", "https://", 1) } else { file.url.clone() };
            index += 1;
            drive_log(&format!("[CLOUD_DRIVE] xunlei download {index}/{} {} bytes", todo.len(), file.size));
            let descriptor = crate::cloud_drive_core::DownloadDescriptor {
                provider: crate::cloud_drive_core::CloudDriveProvider::Xunlei,
                url,
                file_name: file.name.clone(),
                headers: vec![
                    crate::cloud_drive_core::DownloadHeader { name: "User-Agent".to_owned(), value: user_agent.clone() },
                    crate::cloud_drive_core::DownloadHeader { name: "Referer".to_owned(), value: "https://pan.xunlei.com/".to_owned() },
                ],
                expected_size: Some(file.size),
                expected_sha256: None,
            };
            match crate::cloud_drive_core::download_file_with_client(&client, &descriptor, &directory, 3, |_| {}) {
                Ok(receipt) => receipts.push(serde_json::to_value(receipt).map_err(|error| error.to_string())?),
                Err(error) => skipped.push(skip(error)),
            }
        }

        // Wait for Thunder: a file counts once it has the share's exact size
        // and Thunder's in-progress companion (".<name>.js") is gone.
        let deadline = std::time::Instant::now() + XUNLEI_THUNDER_WAIT;
        let handed_at = std::time::SystemTime::now() - std::time::Duration::from_secs(120);
        let mut logged = false;
        while !pending_in_thunder.is_empty() {
            let mut still = Vec::new();
            for (name, path, size) in pending_in_thunder {
                if let Some(found) = thunder_finished_file(&path, size, handed_at) {
                    let path = found;
                    receipts.push(serde_json::json!({
                        "provider": "xunlei", "path": path.to_string_lossy(), "bytes": size,
                        "sha256": sha256_hex(&path)?, "attempts": 1, "integrityVerified": true,
                    }));
                } else {
                    still.push((name, path, size));
                }
            }
            pending_in_thunder = still;
            if pending_in_thunder.is_empty() {
                break;
            }
            if !logged {
                drive_log(&format!("[CLOUD_DRIVE] xunlei waiting for Thunder: {} files", pending_in_thunder.len()));
                logged = true;
            }
            if std::time::Instant::now() >= deadline {
                for (name, path, size) in pending_in_thunder.drain(..) {
                    skipped.push(serde_json::json!({
                        "name": name, "bytes": size,
                        "reason": format!("已交给迅雷客户端，但 {} 小时内还没下载完；完成后会在 {}", XUNLEI_THUNDER_WAIT.as_secs() / 3600, path.to_string_lossy()),
                    }));
                }
                break;
            }
            std::thread::sleep(std::time::Duration::from_secs(5));
        }
        Ok(serde_json::json!({
            "receipts": receipts,
            "skipped": skipped,
            "handedToThunder": handed_off,
        }))
    })
    .await
    .map_err(|error| format!("Cloud Drive download task failed: {error}"))?
}

/// The file Thunder finished for `expected`: the name itself, or the name
/// Thunder picks when it already knows one ("fish-dataset(1).zip", owner test
/// 2026-10-18), with the exact size, written since the hand-off, and without
/// Thunder's in-progress companion ".<name>.js".
fn thunder_finished_file(expected: &Path, size: u64, since: std::time::SystemTime) -> Option<PathBuf> {
    let directory = expected.parent()?;
    let wanted = expected.file_name()?.to_string_lossy().into_owned();
    let (stem, extension) = match wanted.rfind('.') {
        Some(dot) if dot > 0 => (wanted[..dot].to_owned(), wanted[dot..].to_owned()),
        _ => (wanted.clone(), String::new()),
    };
    let renamed = |name: &str| {
        let Some(rest) = name.strip_prefix(&stem) else { return false };
        let Some(middle) = rest.strip_suffix(&extension) else { return false };
        let middle = middle.trim_start();
        middle.len() > 2
            && middle.starts_with('(')
            && middle.ends_with(')')
            && middle[1..middle.len() - 1].chars().all(|c| c.is_ascii_digit())
    };
    let finished = |path: &Path, require_new: bool| {
        let Ok(metadata) = path.metadata() else { return false };
        let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        metadata.is_file()
            && metadata.len() == size
            && !path.with_file_name(format!(".{name}.js")).exists()
            && (!require_new || metadata.modified().is_ok_and(|modified| modified >= since))
    };
    if finished(expected, false) {
        return Some(expected.to_path_buf());
    }
    std::fs::read_dir(directory)
        .ok()?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path.file_name().is_some_and(|name| renamed(&name.to_string_lossy())) && finished(path, true))
}

/// How long the chat waits for Thunder to finish handed-off files.
const XUNLEI_THUNDER_WAIT: std::time::Duration = std::time::Duration::from_secs(2 * 3600);

/// Thunder's download folder (its own preference), else ~/Downloads.
fn thunder_save_folder() -> PathBuf {
    std::process::Command::new("defaults")
        .args(["read", "com.xunlei.Thunder", "LastSavePath"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|path| path.starts_with('/'))
        .map(PathBuf::from)
        .or_else(|| dirs::download_dir())
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

/// Evaluates an in-page script and waits for its intercepted report.
async fn run_xunlei_script(
    window: &tauri::WebviewWindow,
    result_rx: &mut tokio::sync::mpsc::UnboundedReceiver<String>,
    script: &str,
    seconds: u64,
) -> Result<Value, String> {
    while result_rx.try_recv().is_ok() {}
    window.eval(script).map_err(|_| "无法在迅雷云盘页面里执行操作。".to_owned())?;
    let payload = tokio::time::timeout(std::time::Duration::from_secs(seconds), result_rx.recv())
        .await
        .ok()
        .flatten()
        .ok_or_else(|| "迅雷云盘页面没有及时回应。".to_owned())?;
    serde_json::from_str(&payload).map_err(|_| "迅雷云盘返回的内容无法读取。".to_owned())
}

/// Signs the user in once (Xunlei's own page, AI-OS's store), presses the
/// page's own 转存到云盘, then hands the saved items to the Thunder app.
/// Errors are short notes for the reply; guest downloads still go ahead.
#[allow(clippy::too_many_arguments)]
async fn xunlei_save_and_hand_off(
    app: &tauri::AppHandle,
    window: &tauri::WebviewWindow,
    result_rx: &mut tokio::sync::mpsc::UnboundedReceiver<String>,
    load_rx: &mut tokio::sync::mpsc::UnboundedReceiver<()>,
    page: &Url,
    top_items: &[Value],
    missing: &[XunleiFile],
) -> Result<(), String> {
    if !std::path::Path::new("/Applications/Thunder.app").exists() {
        return Err("这台电脑没有安装迅雷客户端".to_owned());
    }
    // Only top-level items that hold a missing file are handed off.
    let wanted: Vec<&Value> = top_items
        .iter()
        .filter(|item| {
            let name = item.get("name").and_then(Value::as_str).unwrap_or("");
            missing.iter().any(|file| {
                (file.dir.is_empty() && file.name == name) || file.dir == name || file.dir.starts_with(&format!("{name}/"))
            })
        })
        .collect();
    if wanted.is_empty() {
        return Err("没有读到分享的顶层文件".to_owned());
    }

    // 1. Sign-in, kept in this window's persistent store.
    let started = std::time::Instant::now();
    let mut shown = false;
    loop {
        if app.get_webview_window(XUNLEI_WINDOW_LABEL).is_none() {
            return Err("迅雷登录窗口被关闭".to_owned());
        }
        let answer = run_xunlei_script(window, result_rx, &XUNLEI_LOGIN_CHECK_SCRIPT.replace("__RESULT__", XUNLEI_RESULT_PATH), 20).await?;
        if answer.get("loggedIn").and_then(Value::as_bool) == Some(true) {
            break;
        }
        if !shown {
            let _ = window.set_title("登录迅雷 — 只需登录一次，AI-OS 会自动转存并交给迅雷下载");
            let _ = window.center();
            let _ = window.set_skip_taskbar(false);
            let _ = window.set_focus();
            shown = true;
            drive_log("[CLOUD_DRIVE] xunlei waiting for sign-in");
        }
        if started.elapsed() >= BAIDU_LOGIN_TIMEOUT {
            return Err("10 分钟内没有完成迅雷登录".to_owned());
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }
    // 2. Reload signed in, then press the page's own 转存到云盘 and read the
    //    page's own restore answer (share id → new drive id).
    let _ = window.set_position(tauri::PhysicalPosition::new(-32000, -32000));
    let _ = window.set_skip_taskbar(true);
    while load_rx.try_recv().is_ok() {}
    window.navigate(page.clone()).map_err(|_| "无法重新打开迅雷分享页面".to_owned())?;
    let _ = tokio::time::timeout(std::time::Duration::from_secs(60), load_rx.recv()).await;
    drive_log("[CLOUD_DRIVE] xunlei signed in; pressing the page's 转存到云盘");
    let save = XUNLEI_PAGE_SAVE_SCRIPT.replace("__RESULT__", XUNLEI_RESULT_PATH);
    let waited = std::time::Instant::now();
    let mut shown = false;
    let (drive_ids, user_id) = loop {
        if app.get_webview_window(XUNLEI_WINDOW_LABEL).is_none() {
            return Err("迅雷窗口被关闭".to_owned());
        }
        let answer = run_xunlei_script(window, result_rx, &save, 20).await?;
        if let Some(error) = answer.get("error").and_then(Value::as_str) {
            return Err(error.to_owned());
        }
        if let Some(map) = answer.get("traceIds").and_then(Value::as_object) {
            let user_id = answer.get("userId").and_then(Value::as_str).unwrap_or("").to_owned();
            let ids: std::collections::HashMap<String, String> = map
                .iter()
                .filter_map(|(share_id, drive_id)| Some((share_id.clone(), drive_id.as_str()?.to_owned())))
                .collect();
            break (ids, user_id);
        }
        // Xunlei may ask the user to verify before saving: show its page.
        if !shown && waited.elapsed() >= std::time::Duration::from_secs(30) {
            let _ = window.set_title("迅雷云盘 — 如需验证请在此完成，AI-OS 会自动继续");
            let _ = window.center();
            let _ = window.set_skip_taskbar(false);
            let _ = window.set_focus();
            shown = true;
        }
        if waited.elapsed() >= BAIDU_LOGIN_TIMEOUT {
            return Err("迅雷 10 分钟内没有完成转存".to_owned());
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    };
    if user_id.is_empty() {
        return Err("没有读到迅雷用户编号".to_owned());
    }
    let handoff: Vec<Value> = wanted
        .iter()
        .filter_map(|item| {
            let share_file_id = item.get("id")?.as_str()?;
            Some(serde_json::json!({
                "id": drive_ids.get(share_file_id)?,
                "name": item.get("name")?.as_str()?,
                "size": item.get("size").and_then(Value::as_str).unwrap_or("0"),
                "space": "",
            }))
        })
        .collect();
    if handoff.is_empty() {
        return Err("迅雷转存后没有返回新文件".to_owned());
    }
    drive_log(&format!("[CLOUD_DRIVE] xunlei saved; handing {} items to Thunder", handoff.len()));

    // 3. Hand off as the page does on macOS; the user's clipboard text is put
    //    back once Thunder had time to read it.
    use base64::Engine;
    let payload = base64::engine::general_purpose::STANDARD.encode(serde_json::to_string(&handoff).expect("json"));
    let previous_clipboard = std::process::Command::new("pbpaste").output().ok().map(|output| output.stdout);
    write_clipboard(payload.as_bytes())?;
    let target = format!("xunleiapp://mac.thunder/downloadCloudFilesFromClipboard?token=&encode=base64&userId={user_id}");
    let opened = std::process::Command::new("open")
        .arg(&target)
        .status()
        .map_err(|error| format!("无法唤起迅雷客户端：{error}"))?;
    tokio::time::sleep(std::time::Duration::from_secs(20)).await;
    if let Some(previous) = previous_clipboard {
        let _ = write_clipboard(&previous);
    }
    if !opened.success() {
        return Err("无法唤起迅雷客户端".to_owned());
    }
    Ok(())
}

fn write_clipboard(bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    let mut child = std::process::Command::new("pbcopy")
        .stdin(std::process::Stdio::piped())
        .spawn()
        .map_err(|error| format!("无法写入剪贴板：{error}"))?;
    child
        .stdin
        .take()
        .ok_or_else(|| "无法写入剪贴板".to_owned())?
        .write_all(bytes)
        .map_err(|error| format!("无法写入剪贴板：{error}"))?;
    child.wait().map_err(|error| format!("无法写入剪贴板：{error}"))?;
    Ok(())
}

/// Reports whether the user is signed in to Xunlei in this store (Xunlei's
/// web client keeps its session in localStorage "credentials_<client id>").
const XUNLEI_LOGIN_CHECK_SCRIPT: &str = r#"(() => {
  let loggedIn = false;
  try {
    for (const key of Object.keys(localStorage)) {
      if (!key.startsWith("credentials_")) continue;
      const credentials = JSON.parse(localStorage.getItem(key) || "null");
      if (credentials && credentials.access_token) loggedIn = true;
    }
  } catch (error) {}
  location.href = "https://pan.xunlei.com__RESULT__?d=" + encodeURIComponent(JSON.stringify({ loggedIn }));
})();"#;

/// Presses the share page's own 转存到云盘 once (every file is checked by
/// default; destination 我的云盘) and reports the page's own restore answer
/// captured by XUNLEI_CAPTURE_SCRIPT: {traceIds: {share file id: drive id},
/// userId}, {waiting} until then, or {error}. Placeholder: __RESULT__.
const XUNLEI_PAGE_SAVE_SCRIPT: &str = r#"(() => {
  const report = (payload) => {
    location.href = "https://pan.xunlei.com__RESULT__?d=" + encodeURIComponent(JSON.stringify(payload));
  };
  const seen = (window.__aiosXunlei = window.__aiosXunlei || {});
  let userId = "";
  try {
    for (const key of Object.keys(localStorage)) {
      if (!key.startsWith("credentials_")) continue;
      const credentials = JSON.parse(localStorage.getItem(key) || "null");
      if (credentials) userId = String(credentials.sub || credentials.user_id || "");
    }
  } catch (error) {}
  if (seen.restoreError) return report({ error: "迅雷转存失败：" + seen.restoreError });
  if (seen.traceIds) return report({ traceIds: seen.traceIds, userId });
  if (!seen.restorePressed) {
    const button = [...document.querySelectorAll("button")].find((element) => element.textContent.replace(/\s/g, "") === "转存到云盘");
    if (!button) return report({ waiting: true });
    seen.restorePressed = true;
    button.click();
  }
  report({ waiting: true });
})();"#;

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
        drive_log("[CLOUD_DRIVE] bdpan signed in");
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
    fn progress_lines_read_as_chat_stages() {
        assert_eq!(stage_text("[CLOUD_DRIVE] quark save to drive").as_deref(), Some("正在转存到你的网盘"));
        assert_eq!(stage_text("[CLOUD_DRIVE] pan123 download 89925421 bytes").as_deref(), Some("正在下载一个 85.8 MB 的文件"));
        assert_eq!(stage_text("[CLOUD_DRIVE] pan123 download 3/12 89925421 bytes").as_deref(), Some("正在下载第 3/12 个文件（85.8 MB）"));
        assert_eq!(plan_totals("[CLOUD_DRIVE] quark plan 2 files 118979239 bytes"), Some((2, 118979239)));
        assert_eq!(stage_text("[CLOUD_DRIVE] quark plan 2 files 118979239 bytes"), None);
        assert_eq!(
            stage_text("[CLOUD_DRIVE] cloud-drive-quark-authorization waiting for sign-in").as_deref(),
            Some("请在弹出的窗口里登录（只需要登录一次）")
        );
        assert!(stage_text("[CLOUD_DRIVE] xunlei waiting for Thunder: 1 files").unwrap().contains("取回"));
        assert_eq!(stage_text("[CLOUD_DRIVE] baidu 5s drive folder holds 1 of 1"), None);
    }

    #[test]
    fn thunder_renamed_downloads_are_recognised() {
        let directory = std::env::temp_dir().join(format!("ai-os-thunder-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let expected = directory.join("fish-dataset.zip");
        let since = std::time::SystemTime::now() - std::time::Duration::from_secs(5);
        std::fs::write(directory.join("fish-dataset(1).zip"), b"12345").unwrap();
        std::fs::write(directory.join("fish-dataset-old.zip"), b"12345").unwrap();
        assert_eq!(thunder_finished_file(&expected, 5, since), Some(directory.join("fish-dataset(1).zip")));
        assert_eq!(thunder_finished_file(&expected, 6, since), None, "size must match");
        std::fs::write(directory.join(".fish-dataset(1).zip.js"), b"x").unwrap();
        assert_eq!(thunder_finished_file(&expected, 5, since), None, "still downloading");
        let _ = std::fs::remove_dir_all(directory);
    }

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
