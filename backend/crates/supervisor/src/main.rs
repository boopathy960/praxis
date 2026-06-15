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
    /// Rollback history, oldest first / most-recent last. Each failed promotion
    /// pops the last entry, so the supervisor can recover across more than one
    /// bad swap instead of being stranded after the first rollback.
    #[serde(default, alias = "previous_binary", deserialize_with = "de_history")]
    previous_binaries: Vec<String>,
    signature: String,
    promoted_at_ms: u128,
}

/// Accept either the new `previous_binaries` array or a legacy scalar/`null`
/// `previous_binary`, so old manifests still load.
fn de_history<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum History {
        List(Vec<String>),
        One(Option<String>),
    }
    Ok(match History::deserialize(deserializer)? {
        History::List(list) => list,
        History::One(Some(one)) => vec![one],
        History::One(None) => Vec::new(),
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = PathBuf::from(env::var("ASTRA_DATA_DIR").unwrap_or_else(|_| "./data".into()));
    let manifest_path = data_dir.join("asc2").join("active_version.json");
    let fallback_binary = env::var("ASTRA_SERVER_BINARY")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_server_binary());
    let mut active = load_active_version(&manifest_path).unwrap_or_else(|| ActiveVersion {
        active_binary: fallback_binary.to_string_lossy().into_owned(),
        previous_binaries: Vec::new(),
        signature: file_signature(&fallback_binary).unwrap_or_default(),
        promoted_at_ms: now_ms(),
    });
    verify_version(&active)?;
    eprintln!("[sup] starting; active={}", active.active_binary);
    let mut child = launch(&active.active_binary)?;
    let startup_ready = wait_until_ready(Duration::from_secs(30));
    eprintln!("[sup] startup readiness of active = {startup_ready}");
    if !startup_ready {
        child.kill()?;
        child.wait()?;
        active = rollback(&active, &manifest_path)?;
        eprintln!("[sup] STARTUP ROLLBACK -> active={}", active.active_binary);
        child = launch(&active.active_binary)?;
        if !wait_until_ready(Duration::from_secs(30)) {
            return Err("active and rollback Astra server binaries both failed readiness".into());
        }
        eprintln!("[sup] rollback binary is ready");
    }

    let mut failures = 0usize;
    loop {
        thread::sleep(Duration::from_secs(5));
        if let Some(next) = load_active_version(&manifest_path)
            && next.signature != active.signature
        {
            eprintln!("[sup] promotion detected -> active={}", next.active_binary);
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
                eprintln!(
                    "[sup] PROMOTION ROLLBACK -> active={}",
                    active.active_binary
                );
                child = launch(&active.active_binary)?;
                if !wait_until_ready(Duration::from_secs(30)) {
                    return Err("automatic promotion rollback failed readiness".into());
                }
            }
        }
        if child.try_wait()?.is_some() || !ready() {
            failures += 1;
            eprintln!("[sup] health check failed; failures={failures}");
        } else {
            failures = 0;
        }
        if failures >= 3 {
            eprintln!(
                "[sup] HEALTH ROLLBACK after {failures} failures; current active={}",
                active.active_binary
            );
            child.kill().ok();
            child.wait().ok();
            active = rollback(&active, &manifest_path)?;
            eprintln!("[sup] HEALTH ROLLBACK -> active={}", active.active_binary);
            child = launch(&active.active_binary)?;
            if !wait_until_ready(Duration::from_secs(30)) {
                return Err("health-triggered rollback failed readiness".into());
            }
            eprintln!("[sup] health rollback binary is ready");
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
    // A missing manifest is the normal first-run case — fall back silently.
    let content = fs::read_to_string(path).ok()?;
    // Tolerate a leading UTF-8 BOM: some editors and PowerShell's `Set-Content
    // -Encoding utf8` prepend one, and serde_json cannot parse it. Without this,
    // a BOM'd manifest is silently ignored and a real promotion is dropped.
    let content = content.trim_start_matches('\u{feff}');
    match serde_json::from_str(content) {
        Ok(version) => Some(version),
        Err(error) => {
            // The manifest exists but is corrupt — say so loudly rather than
            // silently falling back to the shipped binary and masking it.
            eprintln!(
                "[sup] active_version.json at {} is present but unparseable ({error}); \
                 ignoring it and keeping the current binary",
                path.display()
            );
            None
        }
    }
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
    // Pop the most recent previous binary; the remaining history is preserved so
    // a subsequent failure can roll back again (N-deep), instead of being stranded.
    let mut history = current.previous_binaries.clone();
    let previous = history
        .pop()
        .ok_or("no previous Astra server binary is available for rollback")?;
    let previous_path = PathBuf::from(&previous);
    let rolled_back = ActiveVersion {
        active_binary: previous,
        previous_binaries: history,
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
            previous_binaries: Vec::new(),
            signature: file_signature(&binary).expect("signature"),
            promoted_at_ms: now_ms(),
        };
        verify_version(&version).expect("valid");
        fs::write(&binary, b"tampered").expect("tamper");
        assert!(verify_version(&version).is_err());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn rollback_is_n_deep_and_preserves_history() {
        let dir = temp_dir("rollback");
        fs::create_dir_all(&dir).expect("dir");
        let v1 = dir.join("v1.bin");
        let v2 = dir.join("v2.bin");
        let v3 = dir.join("v3.bin");
        fs::write(&v1, b"v1").expect("v1");
        fs::write(&v2, b"v2").expect("v2");
        fs::write(&v3, b"v3").expect("v3");
        let manifest = dir.join("active_version.json");

        // Active v3 with a two-deep history [v1, v2].
        let version = ActiveVersion {
            active_binary: v3.to_string_lossy().into_owned(),
            previous_binaries: vec![
                v1.to_string_lossy().into_owned(),
                v2.to_string_lossy().into_owned(),
            ],
            signature: file_signature(&v3).expect("signature"),
            promoted_at_ms: now_ms(),
        };

        // First rollback -> v2, history shrinks to [v1].
        let r1 = rollback(&version, &manifest).expect("rollback 1");
        assert_eq!(r1.active_binary, v2.to_string_lossy());
        assert_eq!(r1.previous_binaries, vec![v1.to_string_lossy().to_string()]);
        verify_version(&r1).expect("v2 signature");

        // Second rollback -> v1, history empties (N-deep, not stranded after one).
        let r2 = rollback(&r1, &manifest).expect("rollback 2");
        assert_eq!(r2.active_binary, v1.to_string_lossy());
        assert!(r2.previous_binaries.is_empty());
        verify_version(&r2).expect("v1 signature");

        // No history left -> rollback errors rather than panicking.
        assert!(rollback(&r2, &manifest).is_err());

        // A legacy single `previous_binary` manifest still loads.
        let legacy = format!(
            r#"{{"active_binary":"{}","previous_binary":"{}","signature":"x","promoted_at_ms":1}}"#,
            v3.to_string_lossy().replace('\\', "\\\\"),
            v1.to_string_lossy().replace('\\', "\\\\")
        );
        fs::write(&manifest, legacy).expect("legacy");
        let loaded = load_active_version(&manifest).expect("legacy loads");
        assert_eq!(
            loaded.previous_binaries,
            vec![v1.to_string_lossy().to_string()]
        );

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn load_tolerates_a_utf8_bom_and_flags_corruption() {
        let dir = temp_dir("load");
        fs::create_dir_all(&dir).expect("dir");
        let manifest = dir.join("active_version.json");
        let json = r#"{"active_binary":"a.exe","previous_binary":"b.exe","signature":"deadbeef","promoted_at_ms":1}"#;

        // A BOM-prefixed manifest (as PowerShell's `Set-Content -Encoding utf8`
        // writes) must still load — otherwise a real promotion is silently dropped.
        let mut bom = vec![0xEF, 0xBB, 0xBF];
        bom.extend_from_slice(json.as_bytes());
        fs::write(&manifest, &bom).expect("write bom");
        let loaded = load_active_version(&manifest).expect("BOM manifest must load");
        assert_eq!(loaded.active_binary, "a.exe");
        // The legacy scalar `previous_binary` is migrated into the history vec.
        assert_eq!(loaded.previous_binaries, vec!["b.exe".to_string()]);

        // A corrupt manifest returns None (and logs); a missing one also None.
        fs::write(&manifest, b"{ not json").expect("write garbage");
        assert!(load_active_version(&manifest).is_none());
        assert!(load_active_version(&dir.join("absent.json")).is_none());
        fs::remove_dir_all(dir).ok();
    }
}
