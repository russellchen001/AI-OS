use reqwest::{
    blocking::Client,
    header::{HeaderMap, HeaderName, HeaderValue},
    StatusCode,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs::{self, File},
    io::{Read, Write},
    net::IpAddr,
    path::{Component, Path, PathBuf},
    str::FromStr,
    time::Duration,
};
use url::Url;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum CloudDriveProvider {
    Baidu,
    Quark,
    Aliyun,
    Uc,
    Pan123,
    ChinaMobile,
    ChinaTelecom,
    Xunlei,
    Guangya,
}

impl CloudDriveProvider {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Baidu => "百度网盘",
            Self::Quark => "夸克网盘",
            Self::Aliyun => "阿里云盘",
            Self::Uc => "UC 网盘",
            Self::Pan123 => "123 云盘",
            Self::ChinaMobile => "中国移动云盘",
            Self::ChinaTelecom => "天翼云盘",
            Self::Xunlei => "迅雷云盘",
            Self::Guangya => "光鸭云盘",
        }
    }

    pub fn authorization(self) -> &'static str {
        match self {
            Self::Pan123 | Self::Guangya => "anonymous-first",
            _ => "anonymous-first-with-controlled-webview-fallback",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShareLink {
    pub provider: CloudDriveProvider,
    pub provider_name: String,
    pub canonical_url: String,
    pub share_id: String,
    pub extraction_code: Option<String>,
    pub authorization: String,
}

impl ShareLink {
    pub fn parse(source: &str, explicit_code: Option<&str>) -> Result<Self, String> {
        let url = Url::parse(source.trim())
            .map_err(|_| "Cloud-drive share URL is invalid.".to_owned())?;
        if url.scheme() != "https" || url.port().is_some() || !url.username().is_empty() {
            return Err("Cloud-drive share URL must be ordinary HTTPS.".to_owned());
        }
        let host = url
            .host_str()
            .map(|value| value.to_ascii_lowercase())
            .ok_or_else(|| "Cloud-drive share URL has no host.".to_owned())?;
        let provider = provider_for_host(&host)
            .ok_or_else(|| "Cloud-drive share provider is unsupported.".to_owned())?;
        let share_id = share_id(&url, provider)?;
        let query_code = url
            .query_pairs()
            .find(|(key, _)| matches!(key.as_ref(), "pwd" | "code" | "passcode"))
            .map(|(_, value)| value.into_owned());
        let extraction_code = explicit_code
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .or(query_code);
        if extraction_code
            .as_deref()
            .is_some_and(|value| value.len() > 32 || !value.chars().all(valid_code_character))
        {
            return Err("Cloud-drive extraction code is invalid.".to_owned());
        }
        Ok(Self {
            provider,
            provider_name: provider.display_name().to_owned(),
            canonical_url: url.to_string(),
            share_id,
            extraction_code,
            authorization: provider.authorization().to_owned(),
        })
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ShareEntryKind {
    File,
    Folder,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShareEntry {
    pub id: String,
    pub name: String,
    pub kind: ShareEntryKind,
    pub size: Option<u64>,
    #[serde(default)]
    pub children: Vec<ShareEntry>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "mode", rename_all = "kebab-case")]
pub enum ShareSelection {
    All,
    Ids { ids: Vec<String> },
    NameContains { value: String },
}

pub fn select_share_files(
    entries: &[ShareEntry],
    selection: &ShareSelection,
) -> Result<Vec<ShareEntry>, String> {
    let selected_ids = match selection {
        ShareSelection::Ids { ids } if ids.is_empty() => {
            return Err("At least one shared item must be selected.".to_owned())
        }
        ShareSelection::Ids { ids } => Some(ids.iter().cloned().collect::<HashSet<_>>()),
        _ => None,
    };
    let name_hint = match selection {
        ShareSelection::NameContains { value } if value.trim().is_empty() => {
            return Err("Share selection name cannot be empty.".to_owned())
        }
        ShareSelection::NameContains { value } => Some(value.trim().to_lowercase()),
        _ => None,
    };
    let mut files = Vec::new();
    let mut seen = HashSet::new();
    let mut matched = HashSet::new();
    let mut stack = entries
        .iter()
        .rev()
        .map(|entry| (entry, matches!(selection, ShareSelection::All)))
        .collect::<Vec<_>>();

    while let Some((entry, ancestor_selected)) = stack.pop() {
        if !seen.insert(entry.id.clone()) {
            return Err("Provider returned duplicate shared item ids.".to_owned());
        }
        if seen.len() > 10_000 {
            return Err("Shared item tree exceeds the safety limit.".to_owned());
        }
        let selected_here = ancestor_selected
            || selected_ids
                .as_ref()
                .is_some_and(|ids| ids.contains(&entry.id))
            || name_hint
                .as_ref()
                .is_some_and(|hint| entry.name.to_lowercase().contains(hint));
        if selected_here {
            matched.insert(entry.id.clone());
        }
        match entry.kind {
            ShareEntryKind::File if selected_here => files.push(entry.clone()),
            ShareEntryKind::File => {}
            ShareEntryKind::Folder => {
                for child in entry.children.iter().rev() {
                    stack.push((child, selected_here));
                }
            }
        }
    }

    if let Some(ids) = selected_ids {
        if !ids.is_subset(&matched) {
            return Err("One or more selected shared items no longer exist.".to_owned());
        }
    }
    if files.is_empty() {
        return Err("Share selection did not contain any downloadable files.".to_owned());
    }
    Ok(files)
}

fn provider_for_host(host: &str) -> Option<CloudDriveProvider> {
    match host {
        "pan.baidu.com" | "yun.baidu.com" => Some(CloudDriveProvider::Baidu),
        "pan.quark.cn" => Some(CloudDriveProvider::Quark),
        "www.alipan.com" | "alipan.com" | "www.aliyundrive.com" | "aliyundrive.com" => {
            Some(CloudDriveProvider::Aliyun)
        }
        "drive.uc.cn" => Some(CloudDriveProvider::Uc),
        "www.123pan.com" | "123pan.com" | "www.123pan.cn" | "123pan.cn" | "www.123684.com"
        | "123684.com" => Some(CloudDriveProvider::Pan123),
        "caiyun.139.com" => Some(CloudDriveProvider::ChinaMobile),
        "cloud.189.cn" => Some(CloudDriveProvider::ChinaTelecom),
        "pan.xunlei.com" => Some(CloudDriveProvider::Xunlei),
        "www.guangyapan.com" | "guangyapan.com" => Some(CloudDriveProvider::Guangya),
        _ => None,
    }
}

fn share_id(url: &Url, provider: CloudDriveProvider) -> Result<String, String> {
    let parts = url
        .path_segments()
        .map(|segments| segments.filter(|part| !part.is_empty()).collect::<Vec<_>>())
        .unwrap_or_default();
    let candidate = match provider {
        CloudDriveProvider::Baidu | CloudDriveProvider::Quark => parts
            .windows(2)
            .find(|pair| pair[0] == "s")
            .map(|pair| pair[1].to_owned()),
        CloudDriveProvider::Aliyun => parts
            .windows(2)
            .find(|pair| pair[0] == "s" || pair[0] == "drive")
            .map(|pair| pair[1].to_owned()),
        CloudDriveProvider::ChinaMobile if parts.last() == Some(&"i") => url
            .query_pairs()
            .flat_map(|(key, value)| [key.into_owned(), value.into_owned()])
            .filter(|value| value.len() >= 4)
            .max_by_key(String::len),
        _ => parts.last().map(|value| (*value).to_owned()),
    }
    .filter(|value| (4..=256).contains(&value.len()))
    .filter(|value| value.bytes().all(valid_share_id_byte))
    .ok_or_else(|| "Cloud-drive URL does not contain a valid share id.".to_owned())?;
    Ok(candidate)
}

fn valid_share_id_byte(value: u8) -> bool {
    value.is_ascii_alphanumeric() || matches!(value, b'-' | b'_')
}

fn valid_code_character(value: char) -> bool {
    value.is_ascii_alphanumeric() || matches!(value, '-' | '_')
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadDescriptor {
    pub provider: CloudDriveProvider,
    pub url: String,
    pub file_name: String,
    #[serde(default)]
    pub headers: Vec<DownloadHeader>,
    pub expected_size: Option<u64>,
    pub expected_sha256: Option<String>,
}

impl std::fmt::Debug for DownloadDescriptor {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DownloadDescriptor")
            .field("provider", &self.provider)
            .field("url", &self.url)
            .field("file_name", &self.file_name)
            .field("headers", &"[redacted]")
            .field("expected_size", &self.expected_size)
            .field("expected_sha256", &self.expected_sha256)
            .finish()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadHeader {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DownloadReceipt {
    pub provider: CloudDriveProvider,
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
    pub attempts: u8,
    pub integrity_verified: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub attempt: u8,
    pub bytes_downloaded: u64,
    pub total_bytes: Option<u64>,
}

impl DownloadDescriptor {
    pub fn validate(&self) -> Result<(), String> {
        validate_remote_url(&self.url)?;
        safe_file_name(&self.file_name)?;
        if let Some(hash) = &self.expected_sha256 {
            if hash.len() != 64 || !hash.bytes().all(|value| value.is_ascii_hexdigit()) {
                return Err("Expected SHA-256 is invalid.".to_owned());
            }
        }
        header_map(&self.headers)?;
        Ok(())
    }
}

pub fn download_file(
    descriptor: &DownloadDescriptor,
    destination: &Path,
    retry_limit: u8,
) -> Result<DownloadReceipt, String> {
    download_file_with_progress(descriptor, destination, retry_limit, |_| {})
}

pub fn download_file_with_progress<F>(
    descriptor: &DownloadDescriptor,
    destination: &Path,
    retry_limit: u8,
    mut progress: F,
) -> Result<DownloadReceipt, String>
where
    F: FnMut(DownloadProgress),
{
    descriptor.validate()?;
    fs::create_dir_all(destination)
        .map_err(|error| format!("Unable to create download destination: {error}"))?;
    let destination = destination
        .canonicalize()
        .map_err(|error| format!("Unable to resolve download destination: {error}"))?;
    let target = destination.join(&descriptor.file_name);
    if target.exists() {
        return Err("Download target already exists; overwrite was not authorized.".to_owned());
    }
    let partial = destination.join(format!(".{}.part", descriptor.file_name));
    let client = Client::builder()
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(60 * 30))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 8 {
                attempt.stop()
            } else if validate_remote_url(attempt.url().as_str()).is_ok() {
                attempt.follow()
            } else {
                attempt.stop()
            }
        }))
        .build()
        .map_err(|error| format!("Unable to create download client: {error}"))?;
    let headers = header_map(&descriptor.headers)?;
    let maximum_attempts = retry_limit.clamp(1, 5);
    let mut last_error = String::new();

    for attempt in 1..=maximum_attempts {
        match download_attempt(
            &client,
            descriptor,
            &headers,
            &partial,
            attempt,
            &mut progress,
        ) {
            Ok(()) => {
                let bytes = fs::metadata(&partial)
                    .map_err(|error| format!("Unable to inspect downloaded file: {error}"))?
                    .len();
                if descriptor
                    .expected_size
                    .is_some_and(|expected| expected != bytes)
                {
                    last_error = format!("Downloaded size {bytes} did not match expected size.");
                    continue;
                }
                let sha256 = sha256_file(&partial)?;
                if descriptor
                    .expected_sha256
                    .as_deref()
                    .is_some_and(|expected| !expected.eq_ignore_ascii_case(&sha256))
                {
                    let _ = fs::remove_file(&partial);
                    return Err("Downloaded file failed SHA-256 verification.".to_owned());
                }
                fs::rename(&partial, &target)
                    .map_err(|error| format!("Unable to commit downloaded file: {error}"))?;
                return Ok(DownloadReceipt {
                    provider: descriptor.provider,
                    path: target.to_string_lossy().into_owned(),
                    bytes,
                    sha256,
                    attempts: attempt,
                    integrity_verified: true,
                });
            }
            Err(error) => last_error = error,
        }
    }
    Err(format!(
        "Download failed after {maximum_attempts} attempt(s): {last_error}"
    ))
}

fn download_attempt(
    client: &Client,
    descriptor: &DownloadDescriptor,
    headers: &HeaderMap,
    partial: &Path,
    attempt: u8,
    progress: &mut impl FnMut(DownloadProgress),
) -> Result<(), String> {
    let mut request = client.get(&descriptor.url).headers(headers.clone());
    let existing = fs::metadata(partial).map(|value| value.len()).unwrap_or(0);
    if existing > 0 {
        request = request.header("Range", format!("bytes={existing}-"));
    }
    let mut response = request
        .send()
        .map_err(|error| format!("Cloud-drive download request failed: {error}"))?;
    let resumed = response.status() == StatusCode::PARTIAL_CONTENT;
    if !response.status().is_success() {
        return Err(format!(
            "Cloud-drive download returned HTTP {}.",
            response.status()
        ));
    }
    let mut output = if existing > 0 && resumed {
        fs::OpenOptions::new().append(true).open(partial)
    } else {
        File::create(partial)
    }
    .map_err(|error| format!("Unable to open partial download: {error}"))?;
    let starting_bytes = if resumed { existing } else { 0 };
    let total_bytes = response
        .content_length()
        .map(|remaining| starting_bytes.saturating_add(remaining))
        .or(descriptor.expected_size);
    let mut bytes_downloaded = starting_bytes;
    progress(DownloadProgress {
        attempt,
        bytes_downloaded,
        total_bytes,
    });
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = response
            .read(&mut buffer)
            .map_err(|error| format!("Unable to read download response: {error}"))?;
        if count == 0 {
            break;
        }
        output
            .write_all(&buffer[..count])
            .map_err(|error| format!("Unable to write partial download: {error}"))?;
        bytes_downloaded = bytes_downloaded.saturating_add(count as u64);
        progress(DownloadProgress {
            attempt,
            bytes_downloaded,
            total_bytes,
        });
    }
    output
        .sync_all()
        .map_err(|error| format!("Unable to flush partial download: {error}"))
}

fn validate_remote_url(value: &str) -> Result<Url, String> {
    let url = Url::parse(value).map_err(|_| "Download URL is invalid.".to_owned())?;
    if url.scheme() != "https"
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err("Download URL must be ordinary HTTPS.".to_owned());
    }
    let host = url
        .host_str()
        .ok_or_else(|| "Download URL has no host.".to_owned())?;
    if host.eq_ignore_ascii_case("localhost")
        || host.ends_with(".local")
        || host == "169.254.169.254"
        || IpAddr::from_str(host).is_ok_and(|address| {
            address.is_loopback() || address.is_unspecified() || is_private_ip(address)
        })
    {
        return Err("Download URL points to a local or private network address.".to_owned());
    }
    Ok(url)
}

fn is_private_ip(value: IpAddr) -> bool {
    match value {
        IpAddr::V4(value) => value.is_private() || value.is_link_local(),
        IpAddr::V6(value) => value.is_unique_local() || value.is_unicast_link_local(),
    }
}

fn safe_file_name(value: &str) -> Result<(), String> {
    let mut components = Path::new(value).components();
    if value.trim().is_empty()
        || value.contains('/')
        || value.contains('\\')
        || !matches!(components.next(), Some(Component::Normal(_)))
        || components.next().is_some()
    {
        return Err("Download filename is unsafe.".to_owned());
    }
    Ok(())
}

fn header_map(values: &[DownloadHeader]) -> Result<HeaderMap, String> {
    let mut headers = HeaderMap::new();
    for value in values {
        let name = HeaderName::from_bytes(value.name.as_bytes())
            .map_err(|_| "Download header name is invalid.".to_owned())?;
        if matches!(name.as_str(), "host" | "content-length" | "range") {
            return Err("Download header is controlled by the engine.".to_owned());
        }
        let header_value = HeaderValue::from_str(&value.value)
            .map_err(|_| "Download header value is invalid.".to_owned())?;
        headers.insert(name, header_value);
    }
    Ok(headers)
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("Unable to open download for verification: {error}"))?;
    let mut hash = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| format!("Unable to verify downloaded file: {error}"))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

pub fn destination_path(value: &str) -> Result<PathBuf, String> {
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err("Download destination must be an absolute Mac or mounted NAS path.".to_owned());
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_all_nine_share_providers() {
        let cases = [
            (
                "https://pan.baidu.com/s/1SafeShare_1?pwd=a1b2",
                CloudDriveProvider::Baidu,
            ),
            (
                "https://pan.quark.cn/s/SafeShare_2",
                CloudDriveProvider::Quark,
            ),
            (
                "https://www.alipan.com/s/SafeShare_3",
                CloudDriveProvider::Aliyun,
            ),
            ("https://drive.uc.cn/s/SafeShare_4", CloudDriveProvider::Uc),
            (
                "https://www.123pan.com/s/SafeShare_5",
                CloudDriveProvider::Pan123,
            ),
            (
                "https://caiyun.139.com/m/i?SafeShare_6",
                CloudDriveProvider::ChinaMobile,
            ),
            (
                "https://cloud.189.cn/t/SafeShare_7",
                CloudDriveProvider::ChinaTelecom,
            ),
            (
                "https://pan.xunlei.com/s/SafeShare_8",
                CloudDriveProvider::Xunlei,
            ),
            (
                "https://www.guangyapan.com/s/SafeShare_9",
                CloudDriveProvider::Guangya,
            ),
        ];
        for (url, provider) in cases {
            assert_eq!(ShareLink::parse(url, None).unwrap().provider, provider);
        }
    }

    #[test]
    fn extraction_code_is_explicit_and_hosts_are_exact() {
        let link = ShareLink::parse(
            "https://pan.baidu.com/s/1SafeShare_1?pwd=a1b2",
            Some("z9y8"),
        )
        .unwrap();
        assert_eq!(link.extraction_code.as_deref(), Some("z9y8"));
        assert!(ShareLink::parse("https://pan.baidu.com.evil.test/s/1SafeShare_1", None).is_err());
    }

    #[test]
    fn descriptor_redacts_authorization_and_rejects_unsafe_targets() {
        let descriptor = DownloadDescriptor {
            provider: CloudDriveProvider::Quark,
            url: "https://download.example.test/file.bin".to_owned(),
            file_name: "file.bin".to_owned(),
            headers: vec![DownloadHeader {
                name: "Cookie".to_owned(),
                value: "secret=value".to_owned(),
            }],
            expected_size: None,
            expected_sha256: None,
        };
        assert!(descriptor.validate().is_ok());
        assert!(!format!("{descriptor:?}").contains("secret=value"));
        let mut unsafe_descriptor = descriptor;
        unsafe_descriptor.file_name = "../file.bin".to_owned();
        assert!(unsafe_descriptor.validate().is_err());
    }

    #[test]
    fn selection_expands_folders_and_refuses_stale_ids() {
        let entries = vec![ShareEntry {
            id: "folder-1".to_owned(),
            name: "Videos".to_owned(),
            kind: ShareEntryKind::Folder,
            size: None,
            children: vec![
                ShareEntry {
                    id: "file-1".to_owned(),
                    name: "one.mp4".to_owned(),
                    kind: ShareEntryKind::File,
                    size: Some(10),
                    children: Vec::new(),
                },
                ShareEntry {
                    id: "file-2".to_owned(),
                    name: "notes.txt".to_owned(),
                    kind: ShareEntryKind::File,
                    size: Some(2),
                    children: Vec::new(),
                },
            ],
        }];
        let folder = select_share_files(
            &entries,
            &ShareSelection::Ids {
                ids: vec!["folder-1".to_owned()],
            },
        )
        .unwrap();
        assert_eq!(folder.len(), 2);
        let video = select_share_files(
            &entries,
            &ShareSelection::NameContains {
                value: ".mp4".to_owned(),
            },
        )
        .unwrap();
        assert_eq!(video[0].id, "file-1");
        assert!(select_share_files(
            &entries,
            &ShareSelection::Ids {
                ids: vec!["missing".to_owned()],
            },
        )
        .is_err());
    }
}
