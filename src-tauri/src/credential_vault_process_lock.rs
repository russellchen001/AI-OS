use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;

const LOCK_EXCLUSIVE: i32 = 2;
const O_NOFOLLOW_MACOS: i32 = 0x100;

unsafe extern "C" {
    fn flock(fd: i32, operation: i32) -> i32;
}

pub(super) fn acquire() -> Result<File, String> {
    let home = std::env::var_os("HOME").ok_or_else(|| "Home directory unavailable".to_owned())?;

    let directory = PathBuf::from(home).join("Library/Application Support/AI OS");

    fs::create_dir_all(&directory).map_err(|_| "Vault lock directory unavailable".to_owned())?;

    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(O_NOFOLLOW_MACOS)
        .open(directory.join("credential-vault.lock"))
        .map_err(|_| "Vault lock file unavailable".to_owned())?;

    if unsafe { flock(file.as_raw_fd(), LOCK_EXCLUSIVE) } != 0 {
        return Err("Vault process lock failed".to_owned());
    }

    Ok(file)
}

pub(super) fn revision(file: &mut File) -> Result<u64, String> {
    file.seek(SeekFrom::Start(0))
        .map_err(|_| "Vault revision unavailable".to_string())?;
    let mut bytes = Vec::new();
    file.take(32)
        .read_to_end(&mut bytes)
        .map_err(|_| "Vault revision unavailable".to_string())?;
    if bytes.is_empty() {
        return Ok(0);
    }
    std::str::from_utf8(&bytes)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .ok_or_else(|| "Vault revision invalid".to_string())
}

pub(super) fn advance(file: &mut File) -> Result<u64, String> {
    let next = revision(file)?
        .checked_add(1)
        .ok_or_else(|| "Vault revision exhausted".to_string())?;
    file.seek(SeekFrom::Start(0))
        .map_err(|_| "Vault revision unavailable".to_string())?;
    file.write_all(next.to_string().as_bytes())
        .and_then(|_| file.set_len(next.to_string().len() as u64))
        .and_then(|_| file.sync_data())
        .map_err(|_| "Vault revision update failed".to_string())?;
    Ok(next)
}
