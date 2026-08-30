//! On-disk diagnostics for the authenticated browser lifecycle.
//!
//! The managed browser and its restart recovery both run in the background, so
//! when they fail there is nothing on screen that explains why. This records
//! what actually happened, next to the repository, so a failure can be read
//! back instead of guessed at.
//!
//! Lifecycle facts only: provider ids, stages, outcomes and AI-OS error text.
//! Never an account name or marker, never a cookie, token or credential, never
//! a raw profile path and never a control port.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;

fn log_path() -> PathBuf {
    // Compile-time repository location, so the log is always findable even
    // though the app's working directory is not.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|root| root.join("browser-diagnostics.log"))
        .unwrap_or_else(|| PathBuf::from("browser-diagnostics.log"))
}

pub(crate) fn record(stage: &str, detail: &str) {
    let line = format!(
        "{} [{stage}] {detail}\n",
        chrono::Utc::now().to_rfc3339()
    );
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_path()) {
        let _ = file.write_all(line.as_bytes());
    }
}
