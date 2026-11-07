#[path = "../src/credential_vault.rs"]
mod credential_vault;

use credential_vault::CredentialVault;
use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};
use std::sync::Mutex;

static KEYCHAIN_TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn isolated_restart_helper() {
    let Ok(service) = std::env::var("AIOS_B7_RESTART_TEST_SERVICE") else {
        return;
    };
    assert!(service.starts_with("com.ai-os.beta7.test."));
    for _ in 0..20 {
        assert_eq!(
            CredentialVault::read_or_migrate_keychain_entry_to(
                "provider/test",
                "com.ai-os.beta7.test.unused",
                "test",
                &service,
                "temporary-vault"
            )
            .unwrap(),
            Some(b"test".to_vec())
        );
    }
    let report = credential_vault::get_credential_vault_diagnostics();
    let summary = serde_json::json!({"pid":report["pid"],"reads":report["readCalls"],"writes":report["writeCalls"],"legacyReads":report["legacyReadCalls"]});
    std::fs::write(
        std::env::var("AIOS_B7_RESTART_TEST_OUTPUT").unwrap(),
        serde_json::to_vec(&summary).unwrap(),
    )
    .unwrap();
}

#[test]
fn isolated_actual_process_restarts_each_read_keychain_once() {
    let _test_guard = KEYCHAIN_TEST_LOCK.lock().unwrap();
    let service = format!("com.ai-os.beta7.test.{}", uuid::Uuid::new_v4());
    struct Cleanup(String);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = delete_generic_password(&self.0, "temporary-vault");
        }
    }
    let _cleanup = Cleanup(service.clone());
    let directory = tempfile::tempdir().unwrap();
    let data = serde_json::json!({"version":1,"entries":{"provider/test":[116,101,115,116]}});
    set_generic_password(
        &service,
        "temporary-vault",
        &serde_json::to_vec(&data).unwrap(),
    )
    .unwrap();
    let mut pids = Vec::new();
    for index in 0..2 {
        let output = directory.path().join(format!("counts-{index}.json"));
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "isolated_restart_helper", "--nocapture"])
            .env("AIOS_B7_RESTART_TEST_SERVICE", &service)
            .env("AIOS_B7_RESTART_TEST_OUTPUT", &output)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
        let counts: serde_json::Value =
            serde_json::from_slice(&std::fs::read(output).unwrap()).unwrap();
        assert_eq!(counts["reads"], 1);
        assert_eq!(counts["writes"], 0);
        assert_eq!(counts["legacyReads"], 0);
        pids.push(counts["pid"].as_u64().unwrap());
    }
    assert_ne!(pids[0], pids[1]);
}

#[test]
fn isolated_vault_subsystems_share_one_actual_keychain_read() {
    let _test_guard = KEYCHAIN_TEST_LOCK.lock().unwrap();
    let service = format!("com.ai-os.beta7.test.{}", uuid::Uuid::new_v4());
    let account = "temporary-vault";
    struct Cleanup(String);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = delete_generic_password(&self.0, "temporary-vault");
        }
    }
    let _cleanup = Cleanup(service.clone());
    let data = serde_json::json!({"version":1,"entries":{
        "provider/test":[116,101,115,116],"openclaw/test":[116,101,115,116],"download/test":[116,101,115,116]
    }});
    set_generic_password(&service, account, &serde_json::to_vec(&data).unwrap()).unwrap();
    std::thread::scope(|scope| {
        for _ in 0..8 {
            let service = &service;
            scope.spawn(move || {
                for _ in 0..10 {
                    for key in ["provider/test", "openclaw/test", "download/test"] {
                        assert_eq!(
                            CredentialVault::read_or_migrate_keychain_entry_to(
                                key,
                                "com.ai-os.beta7.test.never-used",
                                "test",
                                service,
                                account
                            )
                            .unwrap(),
                            Some(b"test".to_vec())
                        );
                    }
                }
            });
        }
    });
    CredentialVault::update_keychain_entry_to("provider/test", b"test", &service, account).unwrap();
    let report = credential_vault::get_credential_vault_diagnostics();
    let events = report["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["service"] == service)
        .collect::<Vec<_>>();
    assert_eq!(
        events
            .iter()
            .filter(|e| e["operation"] == "READ_BEGIN")
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| e["operation"] == "UPSERT_BEGIN")
            .count(),
        0
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| e["operation"] == "LEGACY_READ_BEGIN")
            .count(),
        0
    );
    assert!(events
        .iter()
        .all(|e| e.get("account").is_none() && e.get("secret").is_none()));
}

#[test]
fn isolated_keychain_round_trip() {
    let _test_guard = KEYCHAIN_TEST_LOCK.lock().unwrap();
    let service = format!("com.ai-os.beta7.test.{}", uuid::Uuid::new_v4());
    let account = "temporary-vault";

    struct Cleanup(String);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = delete_generic_password(&self.0, "temporary-vault");
        }
    }
    let _cleanup = Cleanup(service.clone());

    let mut data = serde_json::to_value(CredentialVault::empty()).unwrap();
    data["entries"]["provider/test"] = serde_json::json!([116, 101, 115, 116]);
    let first = serde_json::to_vec(&data).unwrap();

    set_generic_password(&service, account, &first).unwrap();
    assert_eq!(get_generic_password(&service, account).unwrap(), first);

    data["entries"]["provider/test"] = serde_json::json!([110, 101, 119]);
    let second = serde_json::to_vec(&data).unwrap();

    set_generic_password(&service, account, &second).unwrap();
    let saved = get_generic_password(&service, account).unwrap();
    let vault: CredentialVault = serde_json::from_slice(&saved).unwrap();
    assert!(vault.validate().is_ok());
    assert_eq!(saved, second);

    delete_generic_password(&service, account).unwrap();
    assert_eq!(
        get_generic_password(&service, account).unwrap_err().code(),
        -25300
    );
}

#[test]
fn rejected_vault_writes_preserve_existing_keychain_value() {
    let _test_guard = KEYCHAIN_TEST_LOCK.lock().unwrap();
    let service = format!("com.ai-os.beta7.test.{}", uuid::Uuid::new_v4());
    let account = "temporary-vault";

    struct Cleanup(String);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = delete_generic_password(&self.0, "temporary-vault");
        }
    }
    let _cleanup = Cleanup(service.clone());

    let original = b"existing-keychain-value".to_vec();
    set_generic_password(&service, account, &original).unwrap();

    let mut invalid_data = serde_json::to_value(CredentialVault::empty()).unwrap();
    invalid_data["version"] = serde_json::json!(99);
    let invalid: CredentialVault = serde_json::from_value(invalid_data).unwrap();
    assert!(invalid.save_keychain_to(&service, account).is_err());
    assert_eq!(get_generic_password(&service, account).unwrap(), original);

    let mut oversized_data = serde_json::to_value(CredentialVault::empty()).unwrap();
    oversized_data["entries"]["provider/test"] = serde_json::to_value(vec![b'x'; 262_145]).unwrap();
    let oversized: CredentialVault = serde_json::from_value(oversized_data).unwrap();
    assert!(oversized.save_keychain_to(&service, account).is_err());
    assert_eq!(get_generic_password(&service, account).unwrap(), original);
}

#[test]
fn legacy_keychain_migration_is_non_destructive() {
    let _test_guard = KEYCHAIN_TEST_LOCK.lock().unwrap();
    let id = uuid::Uuid::new_v4();
    let legacy_service = format!("com.ai-os.beta7.legacy.{id}");
    let vault_service = format!("com.ai-os.beta7.vault.{id}");
    let legacy_account = "legacy-provider";
    let vault_account = "temporary-vault";
    let secret = b"legacy-secret".to_vec();

    struct Cleanup {
        legacy_service: String,
        vault_service: String,
    }
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = delete_generic_password(&self.legacy_service, "legacy-provider");
            let _ = delete_generic_password(&self.vault_service, "temporary-vault");
        }
    }
    let _cleanup = Cleanup {
        legacy_service: legacy_service.clone(),
        vault_service: vault_service.clone(),
    };

    set_generic_password(&legacy_service, legacy_account, &secret).unwrap();

    let migrated = CredentialVault::read_or_migrate_keychain_entry_to(
        "provider/legacy-provider",
        &legacy_service,
        legacy_account,
        &vault_service,
        vault_account,
    )
    .unwrap();

    assert_eq!(migrated, Some(secret.clone()));
    assert_eq!(
        get_generic_password(&legacy_service, legacy_account).unwrap(),
        secret
    );

    let vault = CredentialVault::load_keychain_from(&vault_service, vault_account).unwrap();
    assert_eq!(
        serde_json::to_value(vault).unwrap()["entries"]["provider/legacy-provider"],
        serde_json::json!([108, 101, 103, 97, 99, 121, 45, 115, 101, 99, 114, 101, 116])
    );
}
