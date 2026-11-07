use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[cfg(target_os = "macos")]
#[path = "credential_vault_process_lock.rs"]
mod process_lock;

#[cfg(target_os = "macos")]
const VAULT_SERVICE: &str = "com.ai-os.unified-vault.v1";
#[cfg(target_os = "macos")]
const VAULT_ACCOUNT: &str = "primary";
#[cfg(target_os = "macos")]
const MAX_VAULT_BYTES: usize = 262_144;

#[cfg(target_os = "macos")]
static VAULT_AUDIT: std::sync::Mutex<Vec<serde_json::Value>> = std::sync::Mutex::new(Vec::new());

#[cfg(target_os = "macos")]
static VAULT_COUNTS: [std::sync::atomic::AtomicUsize; 3] = [
    std::sync::atomic::AtomicUsize::new(0),
    std::sync::atomic::AtomicUsize::new(0),
    std::sync::atomic::AtomicUsize::new(0),
];

#[cfg(target_os = "macos")]
fn audit_log() -> Option<&'static std::sync::Mutex<std::fs::File>> {
    static FILE: std::sync::OnceLock<Option<std::sync::Mutex<std::fs::File>>> =
        std::sync::OnceLock::new();
    FILE.get_or_init(|| {
        use std::os::unix::fs::OpenOptionsExt;
        let directory = dirs::home_dir()?.join("Library/Logs/AI OS");
        std::fs::create_dir_all(&directory).ok()?;
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()?
            .as_nanos();
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(directory.join(format!("vault-access-{}-{stamp}.jsonl", std::process::id())))
            .ok()
            .map(std::sync::Mutex::new)
    })
    .as_ref()
}

#[cfg(target_os = "macos")]
fn audit_access(operation: &str, service: &str, status: i32) {
    use std::io::Write;
    let counter = match operation {
        "READ_BEGIN" => Some(0),
        "UPSERT_BEGIN" => Some(1),
        "LEGACY_READ_BEGIN" => Some(2),
        _ => None,
    };
    if let Some(counter) = counter {
        VAULT_COUNTS[counter].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    // Only API metadata: never credential bytes, account identifiers, query contents or errors containing secrets.
    let event = serde_json::json!({
        "timestampMs": std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0),
        "pid": std::process::id(), "executable": std::env::current_exe().ok(),
        "operation": operation, "service": service, "status": status
    });
    if let Ok(mut events) = VAULT_AUDIT.lock() {
        if events.len() < 512 {
            events.push(event.clone());
            if let Some(file) = audit_log() {
                if let Ok(mut file) = file.lock() {
                    let _ = writeln!(file, "{event}");
                }
            }
        }
    }
}

#[tauri::command]
pub(crate) fn get_credential_vault_diagnostics() -> serde_json::Value {
    #[cfg(target_os = "macos")]
    {
        let events = VAULT_AUDIT.lock().map(|v| v.clone()).unwrap_or_default();
        let requirement = std::env::current_exe()
            .ok()
            .and_then(|path| {
                std::process::Command::new("/usr/bin/codesign")
                    .args(["-d", "-r-"])
                    .arg(path)
                    .output()
                    .ok()
            })
            .map(|output| {
                String::from_utf8_lossy(&output.stdout).to_string()
                    + &String::from_utf8_lossy(&output.stderr)
            })
            .and_then(|s| {
                s.lines()
                    .find(|s| s.contains("designated =>"))
                    .map(str::to_owned)
            });
        return serde_json::json!({ "pid": std::process::id(), "executable": std::env::current_exe().ok(),
            "signatureRequirement": requirement, "events": events,
            "readCalls": VAULT_COUNTS[0].load(std::sync::atomic::Ordering::Relaxed),
            "writeCalls": VAULT_COUNTS[1].load(std::sync::atomic::Ordering::Relaxed),
            "legacyReadCalls": VAULT_COUNTS[2].load(std::sync::atomic::Ordering::Relaxed),
            "note": "Counts are actual framework API invocations, not macOS authorization-dialog counts. UPSERT is the framework SecItemAdd/Update operation." });
    }
    #[cfg(not(target_os = "macos"))]
    serde_json::json!({"unsupported": true})
}

#[cfg(target_os = "macos")]
impl CredentialVault {
    pub(crate) fn load_keychain() -> Result<Self, String> {
        Self::load_keychain_from(VAULT_SERVICE, VAULT_ACCOUNT)
    }

    pub(crate) fn load_keychain_from(service: &str, account: &str) -> Result<Self, String> {
        Self::access(service, account, |vault| Ok(vault.clone()))
    }

    fn load_uncached(service: &str, account: &str) -> Result<Self, String> {
        use security_framework::passwords::get_generic_password;
        audit_access("READ_BEGIN", service, 0);
        let result = get_generic_password(service, account);
        audit_access(
            "READ",
            service,
            result.as_ref().err().map_or(0, |e| e.code()),
        );
        match result {
            Ok(bytes) => {
                let vault: Self = serde_json::from_slice(&bytes)
                    .map_err(|_| "Credential vault data is invalid".to_owned())?;
                vault.validate().map_err(str::to_owned)?;
                Ok(vault)
            }
            Err(error) if error.code() == -25300 => Ok(Self::empty()),
            Err(_) => Err("Credential vault access was denied or failed".to_owned()),
        }
    }

    pub(crate) fn save_keychain(&self) -> Result<(), String> {
        self.save_keychain_to(VAULT_SERVICE, VAULT_ACCOUNT)
    }

    pub(crate) fn save_keychain_to(&self, service: &str, account: &str) -> Result<(), String> {
        self.encoded()?;
        Self::access(service, account, |vault| {
            *vault = self.clone();
            Ok(())
        })
    }

    fn encoded(&self) -> Result<Vec<u8>, String> {
        self.validate().map_err(str::to_owned)?;
        let bytes =
            serde_json::to_vec(self).map_err(|_| "Credential vault encoding failed".to_owned())?;
        if bytes.len() > MAX_VAULT_BYTES {
            return Err("Credential vault exceeds storage limit".to_owned());
        }
        Ok(bytes)
    }

    fn save_uncached(&self, service: &str, account: &str) -> Result<(), String> {
        let bytes = self.encoded()?;
        // SecItemAdd/Update preserves the existing item's ACL. Never delete/recreate it.
        audit_access("UPSERT_BEGIN", service, 0);
        let result = security_framework::passwords::set_generic_password(service, account, &bytes);
        audit_access(
            "UPSERT",
            service,
            result.as_ref().err().map_or(0, |e| e.code()),
        );
        result.map_err(|_| "Credential vault write failed".to_owned())
    }

    fn access<R>(
        service: &str,
        account: &str,
        operation: impl FnOnce(&mut Self) -> Result<R, String>,
    ) -> Result<R, String> {
        let mut session = VAULT_SESSION
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| "Credential vault session unavailable".to_string())?;
        let mut guard = process_lock::acquire()?;
        let revision = process_lock::revision(&mut guard)?;
        session.transact(
            (service.to_string(), account.to_string()),
            revision,
            || Self::load_uncached(service, account),
            operation,
            |vault| {
                vault.save_uncached(service, account)?;
                process_lock::advance(&mut guard)
            },
        )
    }
}

const VAULT_VERSION: u32 = 1;

#[cfg(target_os = "macos")]
static VAULT_SESSION: std::sync::OnceLock<std::sync::Mutex<VaultSession>> =
    std::sync::OnceLock::new();

#[derive(Default)]
struct VaultSession {
    values: BTreeMap<(String, String), (u64, CredentialVault)>,
}

impl VaultSession {
    fn transact<R>(
        &mut self,
        key: (String, String),
        revision: u64,
        read: impl FnOnce() -> Result<CredentialVault, String>,
        operation: impl FnOnce(&mut CredentialVault) -> Result<R, String>,
        write: impl FnOnce(&CredentialVault) -> Result<u64, String>,
    ) -> Result<R, String> {
        let original = match self.values.get(&key) {
            Some((cached_revision, value)) if *cached_revision == revision => value.clone(),
            _ => {
                self.values.remove(&key);
                read()?
            }
        };
        let mut candidate = original.clone();
        let result = operation(&mut candidate)?;
        candidate.validate().map_err(str::to_string)?;
        let final_revision = if candidate != original {
            match write(&candidate) {
                Ok(next) => next,
                Err(error) => {
                    self.values.remove(&key);
                    return Err(error);
                }
            }
        } else {
            revision
        };
        self.values.insert(key, (final_revision, candidate));
        Ok(result)
    }
}

#[cfg(target_os = "macos")]
impl CredentialVault {
    pub(crate) fn update_keychain_entry(key: &str, secret: &[u8]) -> Result<(), String> {
        Self::update_keychain_entry_to(key, secret, VAULT_SERVICE, VAULT_ACCOUNT)
    }

    pub(crate) fn update_keychain_entry_to(
        key: &str,
        secret: &[u8],
        service: &str,
        account: &str,
    ) -> Result<(), String> {
        Self::access(service, account, |vault| {
            vault.set_entry(key, secret).map_err(str::to_owned)
        })
    }

    pub(crate) fn read_or_migrate_keychain_entry(
        key: &str,
        legacy_service: &str,
        legacy_account: &str,
    ) -> Result<Option<Vec<u8>>, String> {
        Self::read_or_migrate_keychain_entry_to(
            key,
            legacy_service,
            legacy_account,
            VAULT_SERVICE,
            VAULT_ACCOUNT,
        )
    }

    pub(crate) fn read_or_migrate_keychain_entry_to(
        key: &str,
        legacy_service: &str,
        legacy_account: &str,
        vault_service: &str,
        vault_account: &str,
    ) -> Result<Option<Vec<u8>>, String> {
        use security_framework::passwords::get_generic_password;

        Self::access(vault_service, vault_account, |vault| {
            if let Some(secret) = vault.entry(key) {
                return Ok(Some(secret));
            }

            audit_access("LEGACY_READ_BEGIN", legacy_service, 0);
            let result = get_generic_password(legacy_service, legacy_account);
            audit_access(
                "LEGACY_READ",
                legacy_service,
                result.as_ref().err().map_or(0, |e| e.code()),
            );
            let secret = match result {
                Ok(secret) if !secret.is_empty() => secret,
                Ok(_) => return Err("Legacy credential is empty".to_owned()),
                Err(error) if error.code() == -25300 => return Ok(None),
                Err(_) => return Err("Legacy credential access was denied or failed".to_owned()),
            };

            vault.set_entry(key, &secret).map_err(str::to_owned)?;
            Ok(Some(secret))
        })
    }

    pub(crate) fn remove_keychain_entry(key: &str) -> Result<(), String> {
        Self::access(VAULT_SERVICE, VAULT_ACCOUNT, |vault| {
            vault.remove_entry(key);
            Ok(())
        })
    }
}

impl CredentialVault {
    fn entry(&self, key: &str) -> Option<Vec<u8>> {
        self.entries.get(key).cloned()
    }

    fn set_entry(&mut self, key: &str, secret: &[u8]) -> Result<(), &'static str> {
        let mut updated = self.clone();
        updated.entries.insert(key.to_owned(), secret.to_vec());
        updated.validate()?;
        *self = updated;
        Ok(())
    }

    fn remove_entry(&mut self, key: &str) -> bool {
        self.entries.remove(key).is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct CredentialVault {
    version: u32,
    entries: BTreeMap<String, Vec<u8>>,
}

impl CredentialVault {
    pub(crate) fn empty() -> Self {
        Self {
            version: VAULT_VERSION,
            entries: BTreeMap::new(),
        }
    }

    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.version != VAULT_VERSION {
            return Err("Unsupported credential vault version");
        }

        for (key, secret) in &self.entries {
            let valid = ["provider/", "oauth/", "openclaw/", "download/"]
                .iter()
                .any(|prefix| {
                    key.strip_prefix(prefix).is_some_and(|id| {
                        !id.is_empty()
                            && id.len() <= 128
                            && id.bytes().all(|b| {
                                b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')
                            })
                    })
                });

            if !valid || secret.is_empty() {
                return Err("Invalid credential vault entry");
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_session_reads_once_across_subsystems_and_restart_reads_again() {
        use std::cell::Cell;
        let reads = Cell::new(0);
        let key = ("isolated-vault".to_string(), "test".to_string());
        let mut persisted = CredentialVault::empty();
        for name in ["provider/test", "openclaw/test", "download/test"] {
            persisted.set_entry(name, b"fixture").unwrap();
        }
        let read = || {
            reads.set(reads.get() + 1);
            Ok(persisted.clone())
        };
        let mut session = VaultSession::default();
        for _ in 0..20 {
            for name in ["provider/test", "openclaw/test", "download/test"] {
                assert_eq!(
                    session
                        .transact(
                            key.clone(),
                            0,
                            read,
                            |v| Ok(v.entry(name)),
                            |_| panic!("read must not write")
                        )
                        .unwrap(),
                    Some(b"fixture".to_vec())
                );
            }
        }
        assert_eq!(reads.get(), 1);
        let mut restarted = VaultSession::default();
        restarted
            .transact(key, 0, read, |_| Ok(()), |_| panic!("read must not write"))
            .unwrap();
        assert_eq!(reads.get(), 2);
    }

    #[test]
    fn vault_session_denial_and_failed_writes_are_not_cached_as_success() {
        let key = ("isolated-vault".into(), "test".into());
        let mut session = VaultSession::default();
        assert!(session
            .transact(
                key.clone(),
                0,
                || Err("denied".into()),
                |_| Ok(()),
                |_| Ok(1)
            )
            .is_err());
        assert!(session.values.is_empty());
        assert!(session
            .transact(
                key.clone(),
                0,
                || Ok(CredentialVault::empty()),
                |v| v.set_entry("provider/test", b"new").map_err(str::to_string),
                |_| Err("write failed".into())
            )
            .is_err());
        assert!(session.values.is_empty());
        let loaded = session
            .transact(
                key,
                0,
                || Ok(CredentialVault::empty()),
                |v| Ok(v.entry("provider/test")),
                |_| panic!("must not write"),
            )
            .unwrap();
        assert_eq!(loaded, None);
    }

    #[test]
    fn vault_session_unchanged_values_do_not_write_and_external_revision_reloads() {
        let key = ("isolated-vault".into(), "test".into());
        let mut session = VaultSession::default();
        let mut original = CredentialVault::empty();
        original.set_entry("provider/test", b"old").unwrap();
        session
            .transact(
                key.clone(),
                0,
                || Ok(original),
                |v| v.set_entry("provider/test", b"old").map_err(str::to_string),
                |_| panic!("unchanged value must not write"),
            )
            .unwrap();
        session
            .transact(
                key.clone(),
                0,
                || panic!("must reuse successful read"),
                |v| v.set_entry("provider/test", b"new").map_err(str::to_string),
                |_| Ok(1),
            )
            .unwrap();
        assert_eq!(
            session
                .transact(
                    key.clone(),
                    1,
                    || panic!("write-through cache expected"),
                    |v| Ok(v.entry("provider/test")),
                    |_| panic!("must not write")
                )
                .unwrap(),
            Some(b"new".to_vec())
        );
        let mut external = CredentialVault::empty();
        external.set_entry("provider/test", b"external").unwrap();
        assert_eq!(
            session
                .transact(
                    key,
                    2,
                    || Ok(external),
                    |v| Ok(v.entry("provider/test")),
                    |_| panic!("must not write")
                )
                .unwrap(),
            Some(b"external".to_vec())
        );
    }

    #[test]
    fn safe_update_preserves_accounts_and_rejects_invalid_changes() {
        let mut vault = CredentialVault::empty();
        vault.set_entry("provider/openai", b"old").unwrap();
        vault.set_entry("provider/grok", b"grok").unwrap();
        vault.set_entry("provider/openai", b"new").unwrap();
        assert_eq!(vault.entries["provider/openai"], b"new");
        assert_eq!(vault.entries["provider/grok"], b"grok");
        let before = vault.entries.clone();
        assert!(vault.set_entry("unknown/account", b"x").is_err());
        assert!(vault.set_entry("provider/openai", b"").is_err());
        assert_eq!(vault.entries, before);
    }

    #[test]
    fn vault_round_trip_preserves_separate_accounts() {
        let mut vault = CredentialVault::empty();
        vault
            .entries
            .insert("provider/openai".into(), b"token-a".to_vec());
        vault
            .entries
            .insert("openclaw/managed".into(), b"token-b".to_vec());

        let encoded = serde_json::to_vec(&vault).unwrap();
        let restored: CredentialVault = serde_json::from_slice(&encoded).unwrap();

        assert!(restored.validate().is_ok());
        assert_eq!(restored.entries, vault.entries);
    }

    #[test]
    fn vault_rejects_invalid_entries_and_versions() {
        let mut vault = CredentialVault::empty();

        vault.version = 99;
        assert!(vault.validate().is_err());

        vault.version = VAULT_VERSION;
        vault
            .entries
            .insert("unknown/account".into(), b"token".to_vec());
        assert!(vault.validate().is_err());

        vault.entries.clear();
        vault.entries.insert("provider/openai".into(), Vec::new());
        assert!(vault.validate().is_err());

        vault.entries.clear();
        vault
            .entries
            .insert("download/thunder-managed".into(), b"credential".to_vec());
        assert!(vault.validate().is_ok());
    }
}
