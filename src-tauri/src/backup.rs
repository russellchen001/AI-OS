use chrono::{
    DateTime,
    Local,
    Utc,
};

use flate2::{
    read::GzDecoder,
    write::GzEncoder,
    Compression,
};

use serde::{
    Deserialize,
    Serialize,
};

use std::{
    fs::{
        self,
        File,
    },
    io::{
        Read,
        Write,
    },
    path::{
        Path,
        PathBuf,
    },
    process::Command,
};

use tar::{
    Archive,
    Builder,
};

use tempfile::tempdir;

const BACKUP_PREFIX: &str =
    "ai-os-backup-";

const BACKUP_SUFFIX: &str =
    ".tar.gz";

const SETTINGS_FILE: &str =
    "ai-os-settings.json";

const MANIFEST_FILE: &str =
    "backup-manifest.json";

#[derive(
    Debug,
    Deserialize,
)]
#[serde(rename_all = "camelCase")]
pub struct CreateBackupRequest {
    pub destination_directory: String,
    pub include_open_claw_config: bool,
    pub include_ai_os_settings: bool,
    pub settings_json: String,
}

#[derive(
    Debug,
    Deserialize,
)]
#[serde(rename_all = "camelCase")]
pub struct RestoreBackupRequest {
    pub archive_path: String,
    pub restore_open_claw_config: bool,
    pub restore_ai_os_settings: bool,
}

#[derive(
    Debug,
    Serialize,
)]
#[serde(rename_all = "camelCase")]
pub struct BackupResult {
    pub success: bool,
    pub message: String,

    #[serde(
        skip_serializing_if = "Option::is_none"
    )]
    pub archive_path: Option<String>,

    #[serde(
        skip_serializing_if = "Option::is_none"
    )]
    pub restored_settings_json:
        Option<String>,
}

#[derive(
    Debug,
    Serialize,
)]
#[serde(rename_all = "camelCase")]
pub struct BackupRecord {
    pub id: String,
    pub file_name: String,
    pub path: String,
    pub created_at: String,
    pub size_bytes: u64,
}

#[derive(
    Debug,
    Serialize,
    Deserialize,
)]
#[serde(rename_all = "camelCase")]
struct BackupManifest {
    version: String,
    created_at: String,
    includes_open_claw_config: bool,
    includes_ai_os_settings: bool,
    open_claw_sources:
        Vec<String>,
}

fn success(
    message: impl Into<String>,
    archive_path: Option<String>,
    restored_settings_json:
        Option<String>,
) -> BackupResult {
    BackupResult {
        success: true,
        message: message.into(),
        archive_path,
        restored_settings_json,
    }
}

fn failure(
    message: impl Into<String>,
) -> BackupResult {
    BackupResult {
        success: false,
        message: message.into(),
        archive_path: None,
        restored_settings_json: None,
    }
}

fn expand_home(
    value: &str,
) -> PathBuf {
    let trimmed =
        value.trim();

    if trimmed == "~" {
        return dirs::home_dir()
            .unwrap_or_else(|| {
                PathBuf::from(trimmed)
            });
    }

    if let Some(rest) =
        trimmed.strip_prefix("~/")
    {
        if let Some(home) =
            dirs::home_dir()
        {
            return home.join(rest);
        }
    }

    PathBuf::from(trimmed)
}

fn open_claw_candidates()
    -> Vec<(
        &'static str,
        PathBuf,
    )>
{
    let Some(home) =
        dirs::home_dir()
    else {
        return Vec::new();
    };

    vec![
        (
            "dot-openclaw",
            home.join(".openclaw"),
        ),
        (
            "application-support-openclaw",
            home.join(
                "Library/Application Support/OpenClaw",
            ),
        ),
        (
            "launch-agent",
            home.join(
                "Library/LaunchAgents/ai.openclaw.gateway.plist",
            ),
        ),
    ]
}

fn append_path(
    builder:
        &mut Builder<GzEncoder<File>>,
    source: &Path,
    archive_name: &Path,
) -> Result<(), String> {
    if source.is_dir() {
        builder
            .append_dir_all(
                archive_name,
                source,
            )
            .map_err(|error| {
                format!(
                    "Unable to archive {}: {}",
                    source.display(),
                    error
                )
            })?;
    } else if source.is_file() {
        builder
            .append_path_with_name(
                source,
                archive_name,
            )
            .map_err(|error| {
                format!(
                    "Unable to archive {}: {}",
                    source.display(),
                    error
                )
            })?;
    }

    Ok(())
}

fn write_text_file(
    path: &Path,
    contents: &str,
) -> Result<(), String> {
    if let Some(parent) =
        path.parent()
    {
        fs::create_dir_all(parent)
            .map_err(|error| {
                format!(
                    "Unable to create {}: {}",
                    parent.display(),
                    error
                )
            })?;
    }

    let mut file =
        File::create(path)
            .map_err(|error| {
                format!(
                    "Unable to create {}: {}",
                    path.display(),
                    error
                )
            })?;

    file.write_all(
        contents.as_bytes(),
    )
    .map_err(|error| {
        format!(
            "Unable to write {}: {}",
            path.display(),
            error
        )
    })
}

#[tauri::command]
pub fn create_backup(
    request: CreateBackupRequest,
) -> BackupResult {
    let destination =
        expand_home(
            &request
                .destination_directory,
        );

    if request
        .destination_directory
        .trim()
        .is_empty()
    {
        return failure(
            "Backup directory is required.",
        );
    }

    if !request
        .include_open_claw_config
        && !request
            .include_ai_os_settings
    {
        return failure(
            "Select at least one item to back up.",
        );
    }

    if let Err(error) =
        fs::create_dir_all(
            &destination,
        )
    {
        return failure(format!(
            "Unable to create backup directory {}: {}",
            destination.display(),
            error
        ));
    }

    let timestamp =
        Local::now().format(
            "%Y%m%d-%H%M%S",
        );

    let file_name = format!(
        "{}{}{}",
        BACKUP_PREFIX,
        timestamp,
        BACKUP_SUFFIX,
    );

    let archive_path =
        destination.join(file_name);

    let staging =
        match tempdir() {
            Ok(value) => value,

            Err(error) => {
                return failure(
                    format!(
                        "Unable to create temporary directory: {}",
                        error
                    ),
                );
            }
        };

    let mut included_sources =
        Vec::new();

    if request
        .include_ai_os_settings
    {
        let settings_path =
            staging
                .path()
                .join(SETTINGS_FILE);

        if let Err(error) =
            write_text_file(
                &settings_path,
                &request
                    .settings_json,
            )
        {
            return failure(error);
        }
    }

    let candidates =
        open_claw_candidates();

    if request
        .include_open_claw_config
    {
        for (
            archive_name,
            source,
        ) in &candidates
        {
            if source.exists() {
                included_sources.push(
                    archive_name
                        .to_string(),
                );
            }
        }
    }

    let manifest =
        BackupManifest {
            version:
                "1.2.0".to_string(),

            created_at:
                Utc::now()
                    .to_rfc3339(),

            includes_open_claw_config:
                request
                    .include_open_claw_config,

            includes_ai_os_settings:
                request
                    .include_ai_os_settings,

            open_claw_sources:
                included_sources,
        };

    let manifest_json =
        match serde_json::to_string_pretty(
            &manifest,
        ) {
            Ok(value) => value,

            Err(error) => {
                return failure(
                    format!(
                        "Unable to create backup manifest: {}",
                        error
                    ),
                );
            }
        };

    if let Err(error) =
        write_text_file(
            &staging
                .path()
                .join(MANIFEST_FILE),
            &manifest_json,
        )
    {
        return failure(error);
    }

    let output =
        match File::create(
            &archive_path,
        ) {
            Ok(file) => file,

            Err(error) => {
                return failure(
                    format!(
                        "Unable to create archive {}: {}",
                        archive_path.display(),
                        error
                    ),
                );
            }
        };

    let encoder =
        GzEncoder::new(
            output,
            Compression::default(),
        );

    let mut builder =
        Builder::new(encoder);

    if let Err(error) =
        append_path(
            &mut builder,
            &staging
                .path()
                .join(MANIFEST_FILE),
            Path::new(MANIFEST_FILE),
        )
    {
        let _ =
            fs::remove_file(
                &archive_path,
            );

        return failure(error);
    }

    if request
        .include_ai_os_settings
    {
        if let Err(error) =
            append_path(
                &mut builder,
                &staging
                    .path()
                    .join(
                        SETTINGS_FILE,
                    ),
                Path::new(
                    SETTINGS_FILE,
                ),
            )
        {
            let _ =
                fs::remove_file(
                    &archive_path,
                );

            return failure(error);
        }
    }

    if request
        .include_open_claw_config
    {
        for (
            archive_name,
            source,
        ) in candidates
        {
            if !source.exists() {
                continue;
            }

            let target =
                PathBuf::from(
                    "openclaw",
                )
                .join(
                    archive_name,
                );

            if let Err(error) =
                append_path(
                    &mut builder,
                    &source,
                    &target,
                )
            {
                let _ =
                    fs::remove_file(
                        &archive_path,
                    );

                return failure(error);
            }
        }
    }

    if let Err(error) =
        builder.finish()
    {
        let _ =
            fs::remove_file(
                &archive_path,
            );

        return failure(
            format!(
                "Unable to finish backup archive: {}",
                error
            ),
        );
    }

    success(
        format!(
            "Backup created successfully: {}",
            archive_path.display(),
        ),
        Some(
            archive_path
                .to_string_lossy()
                .to_string(),
        ),
        None,
    )
}

fn copy_recursively(
    source: &Path,
    destination: &Path,
) -> Result<(), String> {
    if source.is_file() {
        if let Some(parent) =
            destination.parent()
        {
            fs::create_dir_all(parent)
                .map_err(|error| {
                    format!(
                        "Unable to create {}: {}",
                        parent.display(),
                        error
                    )
                })?;
        }

        fs::copy(
            source,
            destination,
        )
        .map_err(|error| {
            format!(
                "Unable to restore {}: {}",
                destination.display(),
                error
            )
        })?;

        return Ok(());
    }

    fs::create_dir_all(
        destination,
    )
    .map_err(|error| {
        format!(
            "Unable to create {}: {}",
            destination.display(),
            error
        )
    })?;

    for entry in
        fs::read_dir(source)
            .map_err(|error| {
                format!(
                    "Unable to read {}: {}",
                    source.display(),
                    error
                )
            })?
    {
        let entry =
            entry.map_err(
                |error| {
                    error.to_string()
                },
            )?;

        copy_recursively(
            &entry.path(),
            &destination.join(
                entry.file_name(),
            ),
        )?;
    }

    Ok(())
}

fn restore_open_claw(
    extracted_root: &Path,
) -> Result<usize, String> {
    let source_root =
        extracted_root
            .join("openclaw");

    if !source_root.exists() {
        return Ok(0);
    }

    let candidates =
        open_claw_candidates();

    let mut restored = 0;

    for (
        archive_name,
        destination,
    ) in candidates
    {
        let source =
            source_root.join(
                archive_name,
            );

        if !source.exists() {
            continue;
        }

        copy_recursively(
            &source,
            &destination,
        )?;

        restored += 1;
    }

    Ok(restored)
}

#[tauri::command]
pub fn restore_backup(
    request: RestoreBackupRequest,
) -> BackupResult {
    let archive_path =
        expand_home(
            &request.archive_path,
        );

    if !archive_path.is_file() {
        return failure(format!(
            "Backup archive does not exist: {}",
            archive_path.display(),
        ));
    }

    if !request
        .restore_open_claw_config
        && !request
            .restore_ai_os_settings
    {
        return failure(
            "Select at least one item to restore.",
        );
    }

    let temporary =
        match tempdir() {
            Ok(value) => value,

            Err(error) => {
                return failure(
                    format!(
                        "Unable to create restore directory: {}",
                        error
                    ),
                );
            }
        };

    let file =
        match File::open(
            &archive_path,
        ) {
            Ok(value) => value,

            Err(error) => {
                return failure(
                    format!(
                        "Unable to open backup archive: {}",
                        error
                    ),
                );
            }
        };

    let decoder =
        GzDecoder::new(file);

    let mut archive =
        Archive::new(decoder);

    if let Err(error) =
        archive.unpack(
            temporary.path(),
        )
    {
        return failure(
            format!(
                "Unable to extract backup archive: {}",
                error
            ),
        );
    }

    let mut restored_parts =
        Vec::new();

    if request
        .restore_open_claw_config
    {
        match restore_open_claw(
            temporary.path(),
        ) {
            Ok(count) if count > 0 => {
                restored_parts.push(
                    format!(
                        "{} OpenClaw item(s)",
                        count
                    ),
                );
            }

            Ok(_) => {
                restored_parts.push(
                    "no OpenClaw files were present"
                        .to_string(),
                );
            }

            Err(error) => {
                return failure(error);
            }
        }
    }

    let restored_settings_json =
        if request
            .restore_ai_os_settings
        {
            let settings_path =
                temporary
                    .path()
                    .join(
                        SETTINGS_FILE,
                    );

            if settings_path.is_file() {
                let mut value =
                    String::new();

                let mut file =
                    match File::open(
                        &settings_path,
                    ) {
                        Ok(file) => file,

                        Err(error) => {
                            return failure(
                                format!(
                                    "Unable to open restored settings: {}",
                                    error
                                ),
                            );
                        }
                    };

                if let Err(error) =
                    file.read_to_string(
                        &mut value,
                    )
                {
                    return failure(
                        format!(
                            "Unable to read restored settings: {}",
                            error
                        ),
                    );
                }

                restored_parts.push(
                    "AI OS settings"
                        .to_string(),
                );

                Some(value)
            } else {
                restored_parts.push(
                    "no AI OS settings were present"
                        .to_string(),
                );

                None
            }
        } else {
            None
        };

    success(
        format!(
            "Restore completed: {}.",
            restored_parts.join(", "),
        ),
        Some(
            archive_path
                .to_string_lossy()
                .to_string(),
        ),
        restored_settings_json,
    )
}

#[tauri::command]
pub fn list_backups(
    directory: String,
) -> Result<
    Vec<BackupRecord>,
    String,
> {
    let directory =
        expand_home(&directory);

    if !directory.exists() {
        return Ok(Vec::new());
    }

    if !directory.is_dir() {
        return Err(format!(
            "Backup path is not a directory: {}",
            directory.display(),
        ));
    }

    let mut records =
        Vec::new();

    for entry in
        fs::read_dir(&directory)
            .map_err(|error| {
                format!(
                    "Unable to read backup directory: {}",
                    error
                )
            })?
    {
        let entry =
            entry.map_err(
                |error| {
                    error.to_string()
                },
            )?;

        let path =
            entry.path();

        if !path.is_file() {
            continue;
        }

        let Some(file_name) =
            path.file_name()
                .and_then(
                    |value| {
                        value.to_str()
                    },
                )
        else {
            continue;
        };

        if !file_name
            .starts_with(
                BACKUP_PREFIX,
            )
            || !file_name
                .ends_with(
                    BACKUP_SUFFIX,
                )
        {
            continue;
        }

        let metadata =
            entry.metadata()
                .map_err(
                    |error| {
                        error.to_string()
                    },
                )?;

        let modified =
            metadata.modified()
                .ok()
                .map(
                    DateTime::<Utc>::from,
                )
                .map(
                    |date| {
                        date.to_rfc3339()
                    },
                )
                .unwrap_or_else(
                    || {
                        Utc::now()
                            .to_rfc3339()
                    },
                );

        records.push(
            BackupRecord {
                id:
                    path.to_string_lossy()
                        .to_string(),

                file_name:
                    file_name.to_string(),

                path:
                    path.to_string_lossy()
                        .to_string(),

                created_at:
                    modified,

                size_bytes:
                    metadata.len(),
            },
        );
    }

    records.sort_by(
        |left, right| {
            right.created_at.cmp(
                &left.created_at,
            )
        },
    );

    Ok(records)
}

#[tauri::command]
pub fn reveal_backup(
    archive_path: String,
) -> Result<String, String> {
    let archive_path =
        expand_home(
            &archive_path,
        );

    if !archive_path.exists() {
        return Err(format!(
            "Backup does not exist: {}",
            archive_path.display(),
        ));
    }

    let status =
        Command::new(
            "/usr/bin/open",
        )
        .arg("-R")
        .arg(&archive_path)
        .status()
        .map_err(|error| {
            format!(
                "Unable to reveal backup: {}",
                error
            )
        })?;

    if !status.success() {
        return Err(format!(
            "Finder returned status {}",
            status
        ));
    }

    Ok(format!(
        "Opened backup location: {}",
        archive_path.display(),
    ))
}

#[tauri::command]
pub fn delete_backup(
    archive_path: String,
) -> Result<String, String> {
    let archive_path =
        expand_home(
            &archive_path,
        );

    if !archive_path.exists() {
        return Err(format!(
            "Backup does not exist: {}",
            archive_path.display(),
        ));
    }

    if !archive_path.is_file() {
        return Err(
            "Only backup archive files can be deleted."
                .to_string(),
        );
    }

    let Some(file_name) =
        archive_path
            .file_name()
            .and_then(
                |value| value.to_str(),
            )
    else {
        return Err(
            "Invalid backup filename."
                .to_string(),
        );
    };

    if !file_name
        .starts_with(
            BACKUP_PREFIX,
        )
        || !file_name
            .ends_with(
                BACKUP_SUFFIX,
            )
    {
        return Err(
            "Refusing to delete a file that is not an AI OS backup."
                .to_string(),
        );
    }

    fs::remove_file(
        &archive_path,
    )
    .map_err(|error| {
        format!(
            "Unable to delete backup: {}",
            error
        )
    })?;

    Ok(format!(
        "Backup deleted: {}",
        file_name,
    ))
}