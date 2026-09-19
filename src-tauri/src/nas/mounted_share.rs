use serde::Serialize;
use std::{collections::HashSet, path::Path, process::Command};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NetworkStorageTarget {
    pub(crate) id: String,
    pub(crate) provider: String,
    pub(crate) protocol: String,
    pub(crate) display_name: String,
    pub(crate) mount_point: String,
    pub(crate) writable: bool,
    pub(crate) capacity_bytes: Option<u64>,
    pub(crate) used_bytes: Option<u64>,
    pub(crate) available_bytes: Option<u64>,
}

pub(crate) trait NetworkStorageProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn discover(&self) -> Result<Vec<NetworkStorageTarget>, String>;
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct MountedNetworkShareProvider;

impl NetworkStorageProvider for MountedNetworkShareProvider {
    fn id(&self) -> &'static str {
        "mounted-network-share"
    }

    fn discover(&self) -> Result<Vec<NetworkStorageTarget>, String> {
        discover_mounted_network_shares()
    }
}

fn discover_mounted_network_shares() -> Result<Vec<NetworkStorageTarget>, String> {
    let output = Command::new("/sbin/mount")
        .output()
        .or_else(|_| Command::new("mount").output())
        .map_err(|error| format!("Unable to inspect mounted filesystems: {error}"))?;

    if !output.status.success() {
        return Err("Unable to inspect mounted filesystems.".to_owned());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut targets = Vec::new();
    let mut seen = HashSet::new();

    for line in stdout.lines() {
        let Some((source, mount_point, filesystem_type)) = parse_mount_line(line) else {
            continue;
        };

        let Some(protocol) = protocol_for_filesystem(&filesystem_type) else {
            continue;
        };

        if !Path::new(&mount_point).is_dir() {
            continue;
        }

        if !seen.insert(mount_point.clone()) {
            continue;
        }

        let (capacity_bytes, used_bytes, available_bytes) = capacity_for_mount(&mount_point);
        let display_name = display_name_for_mount(&mount_point);
        let safe_source = sanitize_source(&source);

        targets.push(NetworkStorageTarget {
            id: stable_target_id(protocol, &safe_source, &mount_point),
            provider: "mounted-network-share".to_owned(),
            protocol: protocol.to_owned(),
            display_name,
            mount_point: mount_point.clone(),
            writable: path_is_writable(&mount_point),
            capacity_bytes,
            used_bytes,
            available_bytes,
        });
    }

    targets.sort_by(|left, right| left.mount_point.cmp(&right.mount_point));
    Ok(targets)
}

fn parse_mount_line(line: &str) -> Option<(String, String, String)> {
    let (source, remainder) = line.split_once(" on ")?;

    // Linux / Unix util-linux format:
    //
    //   server:/share on /mnt/share type nfs4 (rw,...)
    //   //server/share on /mnt/share type cifs (rw,...)
    //
    // Check this form first because it also ends in "(...)" and would
    // otherwise be mistaken for the macOS form below.
    if let Some((mount_point, typed_remainder)) = remainder.split_once(" type ") {
        let filesystem_type = typed_remainder
            .split_whitespace()
            .next()?
            .trim()
            .trim_end_matches(',');

        if !filesystem_type.is_empty() {
            return Some((
                source.trim().to_owned(),
                mount_point.trim().to_owned(),
                filesystem_type.to_owned(),
            ));
        }
    }

    // macOS format:
    //
    //   //user@server/share on /Volumes/Share (smbfs, nodev, nosuid)
    if let Some((mount_point, details)) = remainder.rsplit_once(" (") {
        let filesystem_type = details.trim_end_matches(')').split(',').next()?.trim();

        if !filesystem_type.is_empty() {
            return Some((
                source.trim().to_owned(),
                mount_point.trim().to_owned(),
                filesystem_type.to_owned(),
            ));
        }
    }

    None
}

fn protocol_for_filesystem(filesystem_type: &str) -> Option<&'static str> {
    match filesystem_type.to_ascii_lowercase().as_str() {
        "smbfs" | "cifs" | "smb3" => Some("smb"),
        "nfs" | "nfs4" => Some("nfs"),
        "webdav" | "davfs" | "davfs2" => Some("webdav"),
        "afpfs" => Some("afp"),
        _ => None,
    }
}

fn display_name_for_mount(mount_point: &str) -> String {
    Path::new(mount_point)
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or("Network Storage")
        .to_owned()
}

fn sanitize_source(source: &str) -> String {
    if let Some(rest) = source.strip_prefix("//") {
        if let Some((identity, path)) = rest.split_once('/') {
            let host = identity
                .rsplit_once('@')
                .map(|(_, host)| host)
                .unwrap_or(identity);

            return format!("//{host}/{path}");
        }
    }

    source.to_owned()
}

fn stable_target_id(protocol: &str, source: &str, mount_point: &str) -> String {
    let input = format!("{protocol}\0{source}\0{mount_point}");

    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in input.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }

    format!("mounted-share-{hash:016x}")
}

fn path_is_writable(path: &str) -> bool {
    Command::new("/usr/bin/test")
        .args(["-w", path])
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn capacity_for_mount(path: &str) -> (Option<u64>, Option<u64>, Option<u64>) {
    let Ok(output) = Command::new("/bin/df").args(["-kP", path]).output() else {
        return (None, None, None);
    };

    if !output.status.success() {
        return (None, None, None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let Some(line) = stdout.lines().last() else {
        return (None, None, None);
    };

    let columns: Vec<&str> = line.split_whitespace().collect();
    if columns.len() < 6 {
        return (None, None, None);
    }

    let to_bytes = |value: &str| value.parse::<u64>().ok()?.checked_mul(1024);

    (
        to_bytes(columns[1]),
        to_bytes(columns[2]),
        to_bytes(columns[3]),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_macos_mount_format() {
        let parsed =
            parse_mount_line("//alice@storage.example/data on /Volumes/Data (smbfs, nodev)")
                .unwrap();

        assert_eq!(
            parsed,
            (
                "//alice@storage.example/data".to_owned(),
                "/Volumes/Data".to_owned(),
                "smbfs".to_owned()
            )
        );
    }

    #[test]
    fn parses_linux_mount_format() {
        let parsed =
            parse_mount_line("//storage.example/data on /mnt/data type cifs (rw,relatime)")
                .unwrap();

        assert_eq!(
            parsed,
            (
                "//storage.example/data".to_owned(),
                "/mnt/data".to_owned(),
                "cifs".to_owned()
            )
        );
    }

    #[test]
    fn recognizes_protocols_without_vendor_identity() {
        assert_eq!(protocol_for_filesystem("smbfs"), Some("smb"));
        assert_eq!(protocol_for_filesystem("cifs"), Some("smb"));
        assert_eq!(protocol_for_filesystem("nfs"), Some("nfs"));
        assert_eq!(protocol_for_filesystem("nfs4"), Some("nfs"));
        assert_eq!(protocol_for_filesystem("webdav"), Some("webdav"));
        assert_eq!(protocol_for_filesystem("apfs"), None);
        assert_eq!(protocol_for_filesystem("ext4"), None);
    }

    #[test]
    fn source_sanitization_removes_username() {
        assert_eq!(
            sanitize_source("//alice@storage.example/data"),
            "//storage.example/data"
        );
    }

    #[test]
    fn stable_id_is_deterministic() {
        assert_eq!(
            stable_target_id("smb", "//storage.example/data", "/Volumes/Data"),
            stable_target_id("smb", "//storage.example/data", "/Volumes/Data")
        );

        assert_ne!(
            stable_target_id("smb", "//storage.example/data", "/Volumes/Data"),
            stable_target_id("nfs", "storage.example:/data", "/Volumes/Data")
        );
    }

    #[test]
    #[ignore = "requires AI_OS_NAS_REAL_E2E_ROOT pointing at a mounted network share"]
    fn real_hardware_e2e_discovers_environment_selected_mount() {
        let expected_root = std::env::var("AI_OS_NAS_REAL_E2E_ROOT")
            .expect("AI_OS_NAS_REAL_E2E_ROOT must be set for real NAS E2E");

        let canonical_expected =
            std::fs::canonicalize(&expected_root).expect("real NAS E2E root must exist");

        let targets =
            discover_mounted_network_shares().expect("network storage discovery must succeed");

        let target = targets
            .iter()
            .find(|target| {
                std::fs::canonicalize(&target.mount_point)
                    .map(|candidate| candidate == canonical_expected)
                    .unwrap_or(false)
            })
            .expect("environment-selected NAS root must be dynamically discovered");

        assert!(!target.id.is_empty());
        assert!(!target.display_name.is_empty());
        assert!(matches!(
            target.protocol.as_str(),
            "smb" | "nfs" | "webdav" | "afp"
        ));
        assert!(target.capacity_bytes.is_some());
        assert!(target.available_bytes.is_some());
    }
}
