#[cfg(debug_assertions)]
pub(crate) fn record(
    operation: &str,
    caller: &str,
    service: &str,
    account: &str,
    cache: &str,
) {
    use sha2::{Digest, Sha256};
    use std::time::{SystemTime, UNIX_EPOCH};

    if std::env::var("AI_OS_KEYCHAIN_TRACE").as_deref() != Ok("1") {
        return;
    }

    let account_hash = Sha256::digest(account.as_bytes())[..8]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let executable = std::env::current_exe()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|_| "unavailable".to_owned());
    let timestamp_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default();
    eprintln!(
        "[KEYCHAIN_TRACE] timestamp_ms={timestamp_ms} operation={operation} caller={caller} service={service} account_hash={account_hash} pid={} executable={} cache={cache}",
        std::process::id(),
        executable
    );
}

#[cfg(not(debug_assertions))]
pub(crate) fn record(
    _operation: &str,
    _caller: &str,
    _service: &str,
    _account: &str,
    _cache: &str,
) {
}
