#[path = "../src/credential_vault_process_lock.rs"]
mod process_lock;

use std::fs;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

struct ManagedChild(Child);

impl Drop for ManagedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_until(path: &std::path::Path) -> bool {
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(10) {
        if path.exists() {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    false
}

#[test]
fn process_lock_helper() {
    let Ok(role) = std::env::var("AIOS_B7_LOCK_ROLE") else {
        return;
    };
    let dir = std::path::PathBuf::from(std::env::var("AIOS_B7_LOCK_DIR").unwrap());
    let mut guard = process_lock::acquire().unwrap();

    match role.as_str() {
        "holder" => {
            assert_eq!(process_lock::revision(&mut guard).unwrap(), 0);
            assert_eq!(process_lock::advance(&mut guard).unwrap(), 1);
            assert_eq!(process_lock::advance(&mut guard).unwrap(), 2);
            fs::write(dir.join("holder-ready"), b"ready").unwrap();
            assert!(wait_until(&dir.join("release")));
        }
        "contender" => {
            assert_eq!(process_lock::revision(&mut guard).unwrap(), 2);
            fs::write(dir.join("contender-acquired"), b"yes").unwrap();
        }
        _ => panic!("invalid test role"),
    }
}

#[test]
fn second_process_waits_for_first_process() {
    let dir = std::env::temp_dir().join(format!(
        "aios-b7-lock-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();

    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(dir.clone());

    let spawn = |role: &str| {
        ManagedChild(
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "process_lock_helper", "--nocapture"])
                .env("HOME", &dir)
                .env("AIOS_B7_LOCK_ROLE", role)
                .env("AIOS_B7_LOCK_DIR", &dir)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        )
    };

    let mut holder = spawn("holder");
    assert!(wait_until(&dir.join("holder-ready")), "holder failed");

    let mut contender = spawn("contender");
    thread::sleep(Duration::from_millis(400));

    assert!(
        !dir.join("contender-acquired").exists(),
        "contender acquired lock before holder released it"
    );
    assert!(
        contender.0.try_wait().unwrap().is_none(),
        "contender exited while lock was held"
    );

    fs::write(dir.join("release"), b"go").unwrap();
    assert!(
        wait_until(&dir.join("contender-acquired")),
        "contender could not acquire released lock"
    );

    let deadline = Instant::now() + Duration::from_secs(10);
    for child in [&mut holder, &mut contender] {
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                assert!(status.success(), "child process failed");
                break;
            }
            assert!(Instant::now() < deadline, "child exit timed out");
            thread::sleep(Duration::from_millis(20));
        }
    }
}
