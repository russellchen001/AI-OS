//! What the machine is doing, read from the machine itself.
//!
//! Everything here is a READ. Nothing in this file changes the system, which is
//! why it is also the one part of Computer Control that is portable today: the
//! numbers come from `sysinfo`, which reads macOS, Windows and Linux -- and so
//! HarmonyOS, which is Linux underneath -- through one pure-Rust crate.
//!
//! That portability is only worth having if it is honest. On a system it does
//! not support, `sysinfo` does not fail: it returns EMPTY VALUES. Zero disks,
//! zero memory, no processes. A caller handed that would read a healthy machine
//! with nothing running on it, which is worse than an error. So every entry
//! point here refuses outright when the crate says the system is unsupported,
//! and none of them can return a zero that was never measured.

use crate::system::SystemError;
use serde_json::{json, Value};
use sysinfo::{Disks, Networks, Pid, System};

/// A process list long enough to be useless is not more honest for being whole.
const MAX_PROCESSES: usize = 200;

fn supported() -> Result<(), SystemError> {
    if sysinfo::IS_SUPPORTED_SYSTEM {
        return Ok(());
    }

    Err(SystemError::execution(
        "this build can read no system information on this operating system. It is \
         reported rather than returned as zeroes, because a machine with no disks \
         and no memory is not what was measured -- it is what could not be.",
    ))
}

fn text(value: &std::ffi::OsStr) -> String {
    value.to_string_lossy().into_owned()
}

/// Where the machine keeps things, and how much room is left.
///
/// Reported per mount point and NEVER summed. On APFS several volumes share one
/// container and each reports the container's whole size: this machine lists
/// `/` and `/System/Volumes/Data` at 460 GiB each, and adding them would claim
/// 920 GiB of disk that does not exist. `sharesContainerWith` names the other
/// mount points a volume's figures are indistinguishable from, so a caller can
/// see the overlap rather than discover it by arithmetic.
pub(crate) fn read_storage(_input: &Value) -> Result<Value, SystemError> {
    supported()?;

    let disks = Disks::new_with_refreshed_list();
    let mut volumes = Vec::new();

    for disk in disks.iter() {
        let total = disk.total_space();
        let available = disk.available_space();

        // A mounted volume that reports nothing at all has not been measured.
        // Saying "0 bytes free" about it would be a statement this code cannot
        // support, so it says what is actually true instead.
        let readable = total > 0;

        let shares: Vec<String> = disks
            .iter()
            .filter(|other| {
                other.mount_point() != disk.mount_point()
                    && other.total_space() == total
                    && other.available_space() == available
                    && total > 0
            })
            .map(|other| other.mount_point().to_string_lossy().into_owned())
            .collect();

        let mut volume = json!({
            "name": text(disk.name()),
            "mountPoint": disk.mount_point().to_string_lossy(),
            "fileSystem": text(disk.file_system()),
            "removable": disk.is_removable(),
            "readOnly": disk.is_read_only(),
            "measured": readable,
        });

        if readable {
            volume["totalBytes"] = json!(total);
            volume["availableBytes"] = json!(available);
            volume["usedBytes"] = json!(total.saturating_sub(available));
        } else {
            volume["note"] = json!(
                "this volume reported no size; it is listed because it is mounted, \
                 and its space is not reported because it was not measured"
            );
        }

        if !shares.is_empty() {
            volume["sharesContainerWith"] = json!(shares);
        }

        volumes.push(volume);
    }

    Ok(json!({
        "capability": "system.storage",
        "operationResult": {
            "volumes": volumes,
            "note": "figures are per mount point and must not be added together: \
                     volumes on one container each report the whole container",
        },
        "warnings": [],
    }))
}

/// How busy the processors are.
///
/// Usage is a difference between two samples, so one reading is not a reading:
/// asked once, `sysinfo` reports whatever it had, which is either zero or an
/// average since boot depending on the platform. This takes two samples with
/// the crate's own minimum interval between them, which is why this capability
/// is deliberately slower than the others.
pub(crate) fn read_cpu(_input: &Value) -> Result<Value, SystemError> {
    supported()?;

    let mut system = System::new();
    system.refresh_cpu_usage();
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    system.refresh_cpu_usage();

    let cores: Vec<Value> = system
        .cpus()
        .iter()
        .map(|cpu| {
            json!({
                "name": cpu.name(),
                "usagePercent": (f64::from(cpu.cpu_usage()) * 10.0).round() / 10.0,
            })
        })
        .collect();

    let brand = system
        .cpus()
        .first()
        .map(|cpu| cpu.brand().trim().to_owned())
        .unwrap_or_default();

    Ok(json!({
        "capability": "system.cpu",
        "operationResult": {
            "brand": brand,
            "logicalCores": cores.len(),
            "physicalCores": System::physical_core_count(),
            "usagePercent": (f64::from(system.global_cpu_usage()) * 10.0).round() / 10.0,
            "cores": cores,
            "note": "usage is measured across two samples taken by this call, not \
                     since the machine started",
        },
        "warnings": [],
    }))
}

/// How much memory there is, and how much of it is spoken for.
pub(crate) fn read_memory(_input: &Value) -> Result<Value, SystemError> {
    supported()?;

    let mut system = System::new();
    system.refresh_memory();

    Ok(json!({
        "capability": "system.memory",
        "operationResult": {
            "totalBytes": system.total_memory(),
            "usedBytes": system.used_memory(),
            "availableBytes": system.available_memory(),
            "freeBytes": system.free_memory(),
            "swapTotalBytes": system.total_swap(),
            "swapUsedBytes": system.used_swap(),
            "note": "available is what a new program could get, which is larger than \
                     free: the difference is memory the system would reclaim",
        },
        "warnings": [],
    }))
}

/// The machine's network interfaces, as the machine sees them.
///
/// All of them, with what each one IS rather than a guess at which ones matter.
/// This machine has twenty-five, most of them tunnels and hardware bridges with
/// no traffic; deciding for the caller which are "real" would be a judgement
/// dressed as data, so each one reports its operational state and its addresses
/// and the caller decides.
pub(crate) fn read_network(_input: &Value) -> Result<Value, SystemError> {
    supported()?;

    let networks = Networks::new_with_refreshed_list();
    let mut interfaces: Vec<Value> = networks
        .iter()
        .map(|(name, data)| {
            let addresses: Vec<String> = data
                .ip_networks()
                .iter()
                .map(|network| format!("{}/{}", network.addr, network.prefix))
                .collect();

            json!({
                "name": name,
                "state": format!("{:?}", data.operational_state()).to_lowercase(),
                "macAddress": data.mac_address().to_string(),
                "addresses": addresses,
                "mtu": data.mtu(),
                "receivedBytesTotal": data.total_received(),
                "transmittedBytesTotal": data.total_transmitted(),
            })
        })
        .collect();

    interfaces.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));

    Ok(json!({
        "capability": "system.network",
        "operationResult": {
            "interfaceCount": interfaces.len(),
            "interfaces": interfaces,
            "note": "every interface the machine has, including tunnels and bridges \
                     that carry nothing; `state` says which are passing packets",
        },
        "warnings": [],
    }))
}

fn process_entry(process: &sysinfo::Process, detailed: bool) -> Value {
    let mut entry = json!({
        "pid": process.pid().as_u32(),
        "name": text(process.name()),
        "memoryBytes": process.memory(),
        "status": format!("{:?}", process.status()).to_lowercase(),
    });

    if let Some(parent) = process.parent() {
        entry["parentPid"] = json!(parent.as_u32());
    }

    if detailed {
        entry["virtualMemoryBytes"] = json!(process.virtual_memory());
        entry["startedAtUnixSeconds"] = json!(process.start_time());
        entry["runTimeSeconds"] = json!(process.run_time());

        if let Some(path) = process.exe() {
            entry["executable"] = json!(path.to_string_lossy());
        }

        entry["command"] = json!(process
            .cmd()
            .iter()
            .map(|part| part.to_string_lossy().into_owned())
            .collect::<Vec<_>>());
    }

    entry
}

/// What is running, biggest first.
///
/// Bounded, and it says what it left out. Six hundred processes returned whole
/// is not a more honest answer than two hundred that admits there were six
/// hundred -- it is the same answer, harder to read, and large enough to be cut
/// off somewhere else without saying so.
pub(crate) fn list_processes(input: &Value) -> Result<Value, SystemError> {
    supported()?;

    let limit = match input.get("limit") {
        None | Some(Value::Null) => MAX_PROCESSES,
        Some(Value::Number(number)) => {
            let requested = number.as_u64().unwrap_or(0) as usize;

            if requested == 0 || requested > MAX_PROCESSES {
                return Err(SystemError::invalid(format!(
                    "limit must be between 1 and {MAX_PROCESSES}"
                )));
            }

            requested
        }
        Some(_) => return Err(SystemError::invalid("limit must be a number")),
    };

    let mut system = System::new_all();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let total = system.processes().len();
    let mut processes: Vec<&sysinfo::Process> = system.processes().values().collect();
    processes.sort_by_key(|process| std::cmp::Reverse(process.memory()));

    let listed: Vec<Value> = processes
        .into_iter()
        .take(limit)
        .map(|process| process_entry(process, false))
        .collect();

    let mut warnings: Vec<String> = Vec::new();

    if listed.len() < total {
        warnings.push(format!(
            "{total} processes are running and the {} largest by memory are listed",
            listed.len()
        ));
    }

    Ok(json!({
        "capability": "system.process.list",
        "operationResult": {
            "processCount": total,
            "listed": listed.len(),
            "orderedBy": "memoryBytes descending",
            "processes": listed,
        },
        "warnings": warnings,
    }))
}

/// One process, in full.
pub(crate) fn read_process(input: &Value) -> Result<Value, SystemError> {
    supported()?;

    let pid = input
        .get("pid")
        .and_then(Value::as_u64)
        .and_then(|pid| u32::try_from(pid).ok())
        .ok_or_else(|| SystemError::invalid("system.process.info requires a pid"))?;

    let mut system = System::new();
    let target = Pid::from_u32(pid);
    system.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[target]), true);

    let process = system.process(target).ok_or_else(|| {
        // Nothing is running under that number, which is a fact about the
        // machine and not a fault in the request.
        SystemError::invalid(format!("no process is running with pid {pid}"))
    })?;

    Ok(json!({
        "capability": "system.process.info",
        "operationResult": process_entry(process, true),
        "warnings": [],
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// These read a live machine, so what can be asserted is what must be TRUE
    /// of any machine -- not a number this one happened to report. A test that
    /// pinned 16 GiB of memory would pass here and fail everywhere else, which
    /// is not a test of anything.
    #[test]
    fn what_is_read_holds_together_whatever_machine_reads_it() {
        let storage = read_storage(&json!({})).unwrap();
        let volumes = storage["operationResult"]["volumes"].as_array().unwrap();

        assert!(!volumes.is_empty(), "a running machine has somewhere to keep things");

        for volume in volumes {
            assert!(volume["mountPoint"].as_str().is_some_and(|m| !m.is_empty()));

            if volume["measured"] == json!(true) {
                let total = volume["totalBytes"].as_u64().unwrap();
                let used = volume["usedBytes"].as_u64().unwrap();
                let available = volume["availableBytes"].as_u64().unwrap();

                assert!(total > 0, "a measured volume has a size");
                assert_eq!(used + available, total, "the parts must equal the whole");
            } else {
                // An unmeasured volume must not carry numbers it never had.
                assert!(volume.get("totalBytes").is_none());
                assert!(volume.get("availableBytes").is_none());
            }
        }

        let cpu = read_cpu(&json!({})).unwrap();
        let result = &cpu["operationResult"];
        let cores = result["cores"].as_array().unwrap();

        assert!(!cores.is_empty());
        assert_eq!(cores.len(), result["logicalCores"].as_u64().unwrap() as usize);

        for reading in std::iter::once(&result["usagePercent"]).chain(
            cores.iter().map(|core| &core["usagePercent"]),
        ) {
            let percent = reading.as_f64().unwrap();
            assert!(
                (0.0..=100.0).contains(&percent),
                "a share of a processor is between none and all of it, got {percent}"
            );
        }

        let memory = read_memory(&json!({})).unwrap()["operationResult"].clone();
        let total = memory["totalBytes"].as_u64().unwrap();

        assert!(total > 0, "a running machine has memory");
        assert!(memory["usedBytes"].as_u64().unwrap() <= total);
        assert!(memory["availableBytes"].as_u64().unwrap() <= total);
        assert!(
            memory["freeBytes"].as_u64().unwrap() <= memory["availableBytes"].as_u64().unwrap(),
            "free memory is a subset of what a new program could be given"
        );

        let network = read_network(&json!({})).unwrap()["operationResult"].clone();
        let interfaces = network["interfaces"].as_array().unwrap();

        assert_eq!(
            interfaces.len(),
            network["interfaceCount"].as_u64().unwrap() as usize,
            "the count and the list are the same fact"
        );

        for interface in interfaces {
            assert!(interface["name"].as_str().is_some_and(|name| !name.is_empty()));
            assert!(interface["state"].as_str().is_some());
        }
    }

    #[test]
    fn the_process_list_is_bounded_ordered_and_says_what_it_left_out() {
        let all = list_processes(&json!({})).unwrap();
        let result = &all["operationResult"];
        let listed = result["processes"].as_array().unwrap();

        let total = result["processCount"].as_u64().unwrap() as usize;
        assert!(total > 0, "something is running, this test for one");
        assert!(listed.len() <= MAX_PROCESSES);
        assert_eq!(listed.len(), result["listed"].as_u64().unwrap() as usize);

        // Ordered as claimed, which is the only reason a bounded list is useful:
        // the two hundred it keeps have to be the two hundred worth keeping.
        let sizes: Vec<u64> = listed
            .iter()
            .map(|process| process["memoryBytes"].as_u64().unwrap())
            .collect();

        assert!(
            sizes.windows(2).all(|pair| pair[0] >= pair[1]),
            "the list claims to be ordered by memory and is not"
        );

        // A truncated answer says so; a complete one does not warn about nothing.
        if listed.len() < total {
            assert!(!all["warnings"].as_array().unwrap().is_empty());
        }

        let few = list_processes(&json!({"limit": 3})).unwrap();
        assert_eq!(few["operationResult"]["processes"].as_array().unwrap().len(), 3);

        for (label, request) in [
            ("no processes at all", json!({"limit": 0})),
            ("more than the ceiling", json!({"limit": MAX_PROCESSES + 1})),
            ("not a number", json!({"limit": "lots"})),
        ] {
            let error = list_processes(&request).unwrap_err();
            assert!(error.invalid_request, "{label} should be a request problem");
        }
    }

    #[test]
    fn one_process_can_be_asked_about_by_number() {
        let mine = read_process(&json!({"pid": std::process::id()})).unwrap();
        let result = &mine["operationResult"];

        assert_eq!(result["pid"].as_u64().unwrap() as u32, std::process::id());
        assert!(result["name"].as_str().is_some_and(|name| !name.is_empty()));
        assert!(result["memoryBytes"].as_u64().unwrap() > 0);
        // The detailed shape carries what the list deliberately leaves out.
        assert!(result.get("command").is_some());
        assert!(result.get("runTimeSeconds").is_some());

        // A number nothing is running under is a fact about the machine, and is
        // reported as a request problem rather than as a failure of this code.
        let absent = read_process(&json!({"pid": 4_294_967_294u32})).unwrap_err();
        assert!(absent.invalid_request);

        assert!(read_process(&json!({})).unwrap_err().invalid_request);
    }
}
