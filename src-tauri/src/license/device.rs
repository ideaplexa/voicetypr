use sha2::{Digest, Sha256};
use std::process::Command;

#[cfg(any(target_os = "windows", test))]
use std::process::Output;

/// Generate a unique device hash based on the machine's hardware ID
pub fn get_device_hash() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    return crate::secure_store::windows_identity::device_hash();

    #[cfg(not(target_os = "windows"))]
    {
        get_machine_uuid().map(|id| hash_machine_id(&id))
    }
}

fn hash_machine_id(machine_id: &str) -> String {
    // Hash the machine ID for privacy
    let mut hasher = Sha256::new();
    hasher.update(machine_id.as_bytes());
    let result = hasher.finalize();
    format!("{:x}", result)
}

/// Get the machine's unique identifier based on the platform
#[cfg(not(target_os = "windows"))]
fn get_machine_uuid() -> Result<String, String> {
    #[cfg(target_os = "macos")]
    {
        get_macos_uuid()
    }

    #[cfg(target_os = "linux")]
    {
        get_linux_uuid()
    }
}

#[cfg(any(target_os = "windows", test))]
trait WindowsCommandRunner {
    fn run(&self, program: &str, args: &[&str], timeout_ms: u64) -> Result<Output, String>;
}

#[cfg(any(target_os = "windows", test))]
struct RealWindowsCommandRunner;

#[cfg(any(target_os = "windows", test))]
impl WindowsCommandRunner for RealWindowsCommandRunner {
    fn run(&self, program: &str, args: &[&str], timeout_ms: u64) -> Result<Output, String> {
        use command_group::CommandGroup;
        use std::io::Read;
        use std::process::Stdio;
        use std::sync::mpsc;
        use std::thread;
        use std::time::{Duration, Instant};

        #[cfg(target_os = "windows")]
        const CREATE_NO_WINDOW: u32 = 0x08000000;

        let deadline = Instant::now() + Duration::from_millis(timeout_ms);
        let mut command = Command::new(program);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut group = command.group();
        // The group builder replaces Command's creation flags when adding
        // CREATE_SUSPENDED, so configure hidden-window behavior on it directly.
        #[cfg(target_os = "windows")]
        group.creation_flags(CREATE_NO_WINDOW);
        let mut child = group
            .spawn()
            .map_err(|e| format!("Failed to execute {}: {}", program, e))?;

        let stdout = child
            .inner()
            .stdout
            .take()
            .ok_or_else(|| format!("Failed to capture {} stdout", program))?;
        let stderr = child
            .inner()
            .stderr
            .take()
            .ok_or_else(|| format!("Failed to capture {} stderr", program))?;

        let (stdout_tx, stdout_rx) = mpsc::channel();
        thread::spawn(move || {
            let mut buf = Vec::new();
            let result = stdout.take(65_536).read_to_end(&mut buf).map(|_| buf);
            let _ = stdout_tx.send(result);
        });
        let (stderr_tx, stderr_rx) = mpsc::channel();
        thread::spawn(move || {
            let mut buf = Vec::new();
            let result = stderr.take(65_536).read_to_end(&mut buf).map(|_| buf);
            let _ = stderr_tx.send(result);
        });

        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        thread::spawn(move || {
                            let _ = child.wait();
                        });
                        return Err(format!("Device lookup timed out: {}", program));
                    }
                    thread::sleep(Duration::from_millis(50));
                }
                Err(e) => {
                    let _ = child.kill();
                    return Err(format!("Failed to wait for {}: {}", program, e));
                }
            }
        };

        // A descendant may retain a pipe after the parent exits. Never join a
        // reader without a deadline; kill the entire job/process group instead.
        let output = (|| {
            let stdout = stdout_rx
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|_| format!("Device lookup stdout timed out: {}", program))?
                .map_err(|_| "Device lookup stdout read failed".to_string())?;
            let stderr = stderr_rx
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|_| format!("Device lookup stderr timed out: {}", program))?
                .map_err(|_| "Device lookup stderr read failed".to_string())?;
            Ok(Output {
                status,
                stdout,
                stderr,
            })
        })();
        if output.is_err() {
            let _ = child.kill();
        }

        output
    }
}

#[cfg(target_os = "macos")]
fn get_macos_uuid() -> Result<String, String> {
    // Get hardware UUID on macOS
    let output = Command::new("ioreg")
        .args(["-d2", "-c", "IOPlatformExpertDevice"])
        .output()
        .map_err(|e| format!("Failed to execute ioreg: {}", e))?;

    if !output.status.success() {
        return Err("Failed to get hardware UUID".to_string());
    }

    let output_str = String::from_utf8_lossy(&output.stdout);

    // Parse the UUID from the output
    for line in output_str.lines() {
        if line.contains("IOPlatformUUID") {
            if let Some(uuid_part) = line.split("\"").nth(3) {
                return Ok(uuid_part.to_string());
            }
        }
    }

    Err("Could not find hardware UUID".to_string())
}

#[cfg(target_os = "windows")]
pub(crate) fn discover_windows_device_hashes() -> Result<Vec<String>, String> {
    windows_machine_ids(&RealWindowsCommandRunner)
        .map(|ids| ids.iter().map(|id| hash_machine_id(id)).collect())
}

#[cfg(test)]
fn get_windows_uuid_with_runner(runner: &dyn WindowsCommandRunner) -> Result<String, String> {
    windows_machine_ids(runner).map(|ids| ids[0].clone())
}

#[cfg(any(target_os = "windows", test))]
fn windows_machine_ids(runner: &dyn WindowsCommandRunner) -> Result<Vec<String>, String> {
    let mut ids = Vec::new();
    fn normalize_uuid(value: &str) -> Option<String> {
        let trimmed = value.trim().trim_matches(&['{', '}', '"', '\''][..]);
        if trimmed.is_empty() {
            return None;
        }

        let lower = trimmed.to_ascii_lowercase();
        if lower == "uuid" || lower.contains("to be filled") {
            return None;
        }

        // Reject diagnostics accidentally printed to stdout, placeholder UUIDs,
        // and malformed identifiers before they can become permanent identity.
        let parsed = uuid::Uuid::parse_str(trimmed).ok()?;
        if parsed.is_nil() || parsed.as_bytes().iter().all(|b| *b == 255) {
            return None;
        }
        Some(trimmed.to_ascii_uppercase())
    }

    // Keep the existing path first for backward compatibility.
    // If WMIC is missing/hanging (newer Windows builds), fall back quickly.
    const WMIC_TIMEOUT_MS: u64 = 4_000;
    const PS_TIMEOUT_MS: u64 = 4_000;
    const REG_TIMEOUT_MS: u64 = 2_500;

    if let Ok(output) = runner.run("wmic", &["csproduct", "get", "UUID"], WMIC_TIMEOUT_MS) {
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            for line in output_str.lines().skip(1) {
                if let Some(uuid) = normalize_uuid(line) {
                    log::info!("[DeviceID] Source: wmic (csproduct UUID)");
                    ids.push(uuid);
                    // Older releases hashed WMIC output without case normalization.
                    if line.trim() != ids[0] {
                        ids.push(line.trim().to_string());
                    }
                    break;
                }
            }
        }
    }

    // Modern Windows: query CIM via PowerShell.
    if ids.is_empty() {
        log::debug!("[DeviceID] wmic failed or unavailable, trying PowerShell CIM...");
        if let Ok(output) = runner.run(
            "powershell",
            &[
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "(Get-CimInstance -ClassName Win32_ComputerSystemProduct).UUID",
            ],
            PS_TIMEOUT_MS,
        ) {
            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                for line in output_str.lines() {
                    if let Some(uuid) = normalize_uuid(line) {
                        log::info!("[DeviceID] Source: PowerShell (Get-CimInstance)");
                        ids.push(uuid);
                        break;
                    }
                }
            }
        }
    }

    // Old WMIC-only releases did not uppercase the returned identifier. Include
    // the lowercase representation for authenticated recovery when WMIC is gone.
    if let Some(hardware) = ids.first() {
        let lowercase = hardware.to_ascii_lowercase();
        if !ids.contains(&lowercase) {
            ids.push(lowercase);
        }
    }

    // Collect MachineGuid independently: existing entries may have been written
    // on a launch where hardware discovery failed, even when it succeeds today.
    log::debug!("[DeviceID] Collecting registry MachineGuid compatibility candidate...");
    if let Ok(output) = runner.run(
        "reg",
        &[
            "query",
            "HKLM\\SOFTWARE\\Microsoft\\Cryptography",
            "/v",
            "MachineGuid",
        ],
        REG_TIMEOUT_MS,
    ) {
        if output.status.success() {
            let output_str = String::from_utf8_lossy(&output.stdout);
            for line in output_str.lines() {
                if !line.contains("MachineGuid") {
                    continue;
                }

                if let Some(rest) = line.split("REG_SZ").nth(1) {
                    if let Some(uuid) = normalize_uuid(rest) {
                        log::info!("[DeviceID] Source: Registry (MachineGuid)");
                        if !ids.contains(&uuid) {
                            ids.push(uuid);
                        }
                        break;
                    }
                }
            }
        }
    }

    if ids.is_empty() {
        log::error!("[DeviceID] All sources failed: wmic, PowerShell, and registry");
        Err("Could not determine machine UUID".to_string())
    } else {
        Ok(ids)
    }
}

#[cfg(target_os = "linux")]
fn get_linux_uuid() -> Result<String, String> {
    // Try to read machine-id on Linux
    use std::fs;

    // Try systemd machine-id first
    if let Ok(machine_id) = fs::read_to_string("/etc/machine-id") {
        return Ok(machine_id.trim().to_string());
    }

    // Try dbus machine-id as fallback
    if let Ok(machine_id) = fs::read_to_string("/var/lib/dbus/machine-id") {
        return Ok(machine_id.trim().to_string());
    }

    // As a last resort, try to get the first MAC address
    get_linux_mac_address()
}

#[cfg(target_os = "linux")]
fn get_linux_mac_address() -> Result<String, String> {
    let output = Command::new("ip")
        .args(&["link", "show"])
        .output()
        .map_err(|e| format!("Failed to execute ip command: {}", e))?;

    if !output.status.success() {
        return Err("Failed to get network interfaces".to_string());
    }

    let output_str = String::from_utf8_lossy(&output.stdout);

    // Find the first non-loopback MAC address
    for line in output_str.lines() {
        if line.contains("link/ether") {
            if let Some(mac) = line.split_whitespace().nth(1) {
                // Skip loopback addresses
                if mac != "00:00:00:00:00:00" {
                    return Ok(mac.to_string());
                }
            }
        }
    }

    Err("Could not find a valid MAC address".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_device_hash_consistency() {
        // Test that device hash is consistent across calls
        let hash1 = get_device_hash().expect("Should get device hash");
        let hash2 = get_device_hash().expect("Should get device hash");

        assert_eq!(hash1, hash2, "Device hash should be consistent");
        assert_eq!(hash1.len(), 64, "SHA256 hash should be 64 characters");
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn test_device_hash_format() {
        let hash = get_device_hash().expect("Should get device hash");

        // Check that it's a valid hex string
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(hash.len(), 64);
    }

    #[test]
    fn windows_uuid_falls_back_when_wmic_unavailable() {
        use std::cell::RefCell;
        #[cfg(unix)]
        use std::os::unix::process::ExitStatusExt;
        #[cfg(target_os = "windows")]
        use std::os::windows::process::ExitStatusExt;
        use std::process::ExitStatus;

        struct StubRunner {
            responses: RefCell<Vec<Result<Output, String>>>,
        }

        impl WindowsCommandRunner for StubRunner {
            fn run(
                &self,
                _program: &str,
                _args: &[&str],
                _timeout_ms: u64,
            ) -> Result<Output, String> {
                self.responses.borrow_mut().remove(0)
            }
        }

        let ok = |stdout: &str| Output {
            status: ExitStatus::from_raw(0),
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
        };

        let runner = StubRunner {
            responses: RefCell::new(vec![
                Err("wmic not found".to_string()),
                Ok(ok("{550E8400-E29B-41D4-A716-446655440000}\r\n")),
                Err("registry unavailable".to_string()),
            ]),
        };

        let uuid = get_windows_uuid_with_runner(&runner).expect("should fall back to PowerShell");
        assert_eq!(uuid, "550E8400-E29B-41D4-A716-446655440000");
    }

    #[test]
    fn windows_uuid_falls_back_to_registry_when_powershell_fails() {
        use std::cell::RefCell;
        #[cfg(unix)]
        use std::os::unix::process::ExitStatusExt;
        #[cfg(target_os = "windows")]
        use std::os::windows::process::ExitStatusExt;
        use std::process::ExitStatus;

        struct StubRunner {
            responses: RefCell<Vec<Result<Output, String>>>,
        }

        impl WindowsCommandRunner for StubRunner {
            fn run(
                &self,
                _program: &str,
                _args: &[&str],
                _timeout_ms: u64,
            ) -> Result<Output, String> {
                self.responses.borrow_mut().remove(0)
            }
        }

        let ok = |stdout: &str| Output {
            status: ExitStatus::from_raw(0),
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
        };

        let fail = || Output {
            status: ExitStatus::from_raw(1),
            stdout: Vec::new(),
            stderr: b"error".to_vec(),
        };

        let runner = StubRunner {
            responses: RefCell::new(vec![
                Ok(fail()),
                Ok(fail()),
                Ok(ok(
                    "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Cryptography\r\n    MachineGuid    REG_SZ    123e4567-e89b-12d3-a456-426614174000\r\n",
                )),
            ]),
        };

        let uuid = get_windows_uuid_with_runner(&runner).expect("should fall back to registry");
        assert_eq!(uuid, "123E4567-E89B-12D3-A456-426614174000");
    }

    #[test]
    #[cfg(unix)]
    fn command_timeout_includes_descendants_holding_output_pipes() {
        let start = std::time::Instant::now();
        let result = RealWindowsCommandRunner.run("/bin/sh", &["-c", "sleep 5 & exit 0"], 100);
        assert!(result.unwrap_err().contains("timed out"));
        assert!(start.elapsed() < std::time::Duration::from_secs(2));
    }

    #[test]
    fn discovery_retains_registry_candidate_even_when_hardware_works() {
        #[cfg(unix)]
        use std::os::unix::process::ExitStatusExt;
        #[cfg(target_os = "windows")]
        use std::os::windows::process::ExitStatusExt;
        struct Runner {
            hardware_available: bool,
        }
        impl WindowsCommandRunner for Runner {
            fn run(&self, program: &str, _: &[&str], _: u64) -> Result<Output, String> {
                let stdout = match program {
                    "wmic" if self.hardware_available => {
                        "UUID\n550e8400-e29b-41d4-a716-446655440000\n"
                    }
                    "reg" => "MachineGuid REG_SZ 123e4567-e89b-12d3-a456-426614174000\n",
                    // Successful process exit is not proof of a usable UUID.
                    _ => "WARNING: hardware lookup failed\n00000000-0000-0000-0000-000000000000\n",
                };
                Ok(Output {
                    status: std::process::ExitStatus::from_raw(0),
                    stdout: stdout.as_bytes().to_vec(),
                    stderr: vec![],
                })
            }
        }
        let available = windows_machine_ids(&Runner {
            hardware_available: true,
        })
        .unwrap();
        let unavailable = windows_machine_ids(&Runner {
            hardware_available: false,
        })
        .unwrap();
        assert_eq!(
            available,
            vec![
                "550E8400-E29B-41D4-A716-446655440000",
                "550e8400-e29b-41d4-a716-446655440000",
                "123E4567-E89B-12D3-A456-426614174000"
            ]
        );
        assert_eq!(unavailable, vec!["123E4567-E89B-12D3-A456-426614174000"]);
        assert_ne!(
            hash_machine_id(&available[0]),
            hash_machine_id(&unavailable[0])
        );
    }

    #[test]
    #[cfg(unix)]
    fn command_captures_both_streams_without_mixing_them() {
        let output = RealWindowsCommandRunner
            .run(
                "/bin/sh",
                &["-c", "printf hardware; printf diagnostic >&2"],
                2_000,
            )
            .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"hardware");
        assert_eq!(output.stderr, b"diagnostic");
    }
}
