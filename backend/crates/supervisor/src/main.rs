use std::env;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha3::{Digest, Sha3_256};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ActiveVersion {
    active_binary: String,
    previous_binary: Option<String>,
    signature: String,
    promoted_at_ms: u128,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = PathBuf::from(env::var("ASTRA_DATA_DIR").unwrap_or_else(|_| "./data".into()));
    let manifest_path = data_dir.join("asc2").join("active_version.json");
    let fallback_binary = env::var("ASTRA_SERVER_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_server_binary());
    let mut active = load_active_version(&manifest_path).unwrap_or_else(|| ActiveVersion {
        active_binary: fallback_binary.to_string_lossy().into_owned(),
        previous_binary: None,
        signature: file_signature(&fallback_binary).unwrap_or_default(),
        promoted_at_ms: now_ms(),
    });
    verify_version(&active)?;
    let mut child = launch(&active.active_binary)?;
    if !wait_until_ready(Duration::from_secs(30)) {
        child.kill()?;
        child.wait()?;
        active = rollback(&active, &manifest_path)?;
        child = launch(&active.active_binary)?;
        if !wait_until_ready(Duration::from_secs(30)) {
            return Err("active and rollback Astra server binaries both failed readiness".into());
        }
    }

    let mut failures = 0usize;
    loop {
        thread::sleep(Duration::from_secs(5));
        if let Some(next) = load_active_version(&manifest_path)
            && next.signature != active.signature
        {
            verify_version(&next)?;
            child.kill()?;
            child.wait()?;
            let candidate = launch(&next.active_binary)?;
            child = candidate;
            if wait_until_ready(Duration::from_secs(30)) {
                active = next;
                failures = 0;
            } else {
                child.kill()?;
                child.wait()?;
                active = rollback(&next, &manifest_path)?;
                child = launch(&active.active_binary)?;
                if !wait_until_ready(Duration::from_secs(30)) {
                    return Err("automatic promotion rollback failed readiness".into());
                }
            }
        }
        if child.try_wait()?.is_some() || !ready() {
            failures += 1;
        } else {
            failures = 0;
        }
        if failures >= 3 {
            child.kill().ok();
            child.wait().ok();
            active = rollback(&active, &manifest_path)?;
            child = launch(&active.active_binary)?;
            if !wait_until_ready(Duration::from_secs(30)) {
                return Err("health-triggered rollback failed readiness".into());
            }
            failures = 0;
        }
    }
}

fn launch(binary: &str) -> Result<Child, Box<dyn std::error::Error>> {
    let path = Path::new(binary);
    if !path.is_file() {
        return Err(format!("Astra server binary does not exist: {}", path.display()).into());
    }
    Ok(Command::new(path).spawn()?)
}

fn load_active_version(path: &Path) -> Option<ActiveVersion> {
    fs::read_to_string(path)
        .ok()
        .and_then(|content| serde_json::from_str(&content).ok())
}

fn verify_version(version: &ActiveVersion) -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new(&version.active_binary);
    let signature = file_signature(path)?;
    if signature != version.signature {
        return Err(format!("binary signature mismatch for {}", path.display()).into());
    }
    Ok(())
}

fn rollback(
    current: &ActiveVersion,
    manifest_path: &Path,
) -> Result<ActiveVersion, Box<dyn std::error::Error>> {
    let previous = current
        .previous_binary
        .as_ref()
        .ok_or("no previous Astra server binary is available for rollback")?;
    let previous_path = PathBuf::from(previous);
    let rolled_back = ActiveVersion {
        active_binary: previous.clone(),
        previous_binary: None,
        signature: file_signature(&previous_path)?,
        promoted_at_ms: now_ms(),
    };
    if let Some(parent) = manifest_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(manifest_path, serde_json::to_vec_pretty(&rolled_back)?)?;
    Ok(rolled_back)
}

fn wait_until_ready(timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if ready() {
            return true;
        }
        thread::sleep(Duration::from_millis(500));
    }
    false
}

fn ready() -> bool {
    let host = env::var("ASTRA_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port = env::var("ASTRA_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8080);
    let address = format!("{host}:{port}");
    let socket = match address
        .to_socket_addrs()
        .ok()
        .and_then(|mut addresses| addresses.next())
    {
        Some(socket) => socket,
        None => return false,
    };
    let mut stream = match TcpStream::connect_timeout(&socket, Duration::from_secs(1)) {
        Ok(stream) => stream,
        Err(_) => return false,
    };
    stream.set_read_timeout(Some(Duration::from_secs(1))).ok();
    if write!(
        stream,
        "GET /api/v1/ready HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    )
    .is_err()
    {
        return false;
    }
    let mut response = String::new();
    stream.read_to_string(&mut response).is_ok()
        && response.starts_with("HTTP/1.1 200")
        && response.contains("\"ok\":true")
}

fn file_signature(path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    let bytes = fs::read(path)?;
    let digest = Sha3_256::digest(bytes);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn default_server_binary() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from("target/release/astra-server.exe")
    } else {
        PathBuf::from("target/release/astra-server")
    }
}

fn now_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(label: &str) -> PathBuf {
        env::temp_dir().join(format!("astra-supervisor-{label}-{}", now_ms()))
    }

    #[test]
    fn binary_signature_detects_tampering() {
        let dir = temp_dir("tamper");
        fs::create_dir_all(&dir).expect("dir");
        let binary = dir.join("server.bin");
        fs::write(&binary, b"version-one").expect("write");
        let version = ActiveVersion {
            active_binary: binary.to_string_lossy().into_owned(),
            previous_binary: None,
            signature: file_signature(&binary).expect("signature"),
            promoted_at_ms: now_ms(),
        };
        verify_version(&version).expect("valid");
        fs::write(&binary, b"tampered").expect("tamper");
        assert!(verify_version(&version).is_err());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn rollback_restores_previous_binary_manifest() {
        let dir = temp_dir("rollback");
        fs::create_dir_all(&dir).expect("dir");
        let current = dir.join("current.bin");
        let previous = dir.join("previous.bin");
        fs::write(&current, b"current").expect("current");
        fs::write(&previous, b"previous").expect("previous");
        let manifest = dir.join("active_version.json");
        let version = ActiveVersion {
            active_binary: current.to_string_lossy().into_owned(),
            previous_binary: Some(previous.to_string_lossy().into_owned()),
            signature: file_signature(&current).expect("signature"),
            promoted_at_ms: now_ms(),
        };
        let restored = rollback(&version, &manifest).expect("rollback");
        assert_eq!(restored.active_binary, previous.to_string_lossy());
        verify_version(&restored).expect("restored signature");
        fs::remove_dir_all(dir).ok();
    }
}
