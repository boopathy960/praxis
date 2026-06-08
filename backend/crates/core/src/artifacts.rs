use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};

use crate::Shared;
use crate::common::{AppError, new_id, now_ms, sha3_hex};

const MAX_SAFE_PREVIEW_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactSafetyStatus {
    Safe,
    Warning,
    QuarantinedWarning,
    Purged,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactSafetyReport {
    pub artifact_id: String,
    pub source_url: Option<String>,
    pub path: String,
    pub content_hash: String,
    pub byte_len: usize,
    pub mime_guess: String,
    pub status: ArtifactSafetyStatus,
    pub risk_score: f64,
    pub warnings: Vec<String>,
    pub scanned_at_ms: i64,
    pub purged_at_ms: Option<i64>,
}

#[derive(Default)]
struct ArtifactSafetyStore {
    reports: HashMap<String, ArtifactSafetyReport>,
}

#[derive(Clone)]
pub struct ArtifactSafetyService {
    store: Shared<ArtifactSafetyStore>,
}

impl ArtifactSafetyService {
    #[must_use]
    pub fn new() -> Self {
        Self {
            store: Shared::new(RwLock::new(ArtifactSafetyStore::default())),
        }
    }

    pub fn scan_file(
        &self,
        artifact_id: &str,
        path: impl Into<PathBuf>,
        source_url: Option<String>,
        declared_content_type: Option<&str>,
    ) -> Result<ArtifactSafetyReport, AppError> {
        let path = path.into();
        let bytes = fs::read(&path).map_err(|error| {
            AppError::Internal(format!("failed to read artifact for safety scan: {error}"))
        })?;
        let report = scan_bytes(
            artifact_id,
            path.to_string_lossy().as_ref(),
            source_url,
            declared_content_type,
            &bytes,
        );
        self.store
            .write()
            .reports
            .insert(artifact_id.to_string(), report.clone());
        Ok(report)
    }

    pub fn record_report(&self, report: ArtifactSafetyReport) {
        self.store
            .write()
            .reports
            .insert(report.artifact_id.clone(), report);
    }

    pub fn get_report(&self, artifact_id: &str) -> Result<ArtifactSafetyReport, AppError> {
        self.store
            .read()
            .reports
            .get(artifact_id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("artifact safety report {artifact_id}")))
    }

    pub fn purge(&self, artifact_id: &str) -> Result<ArtifactSafetyReport, AppError> {
        let mut store = self.store.write();
        let report = store
            .reports
            .get_mut(artifact_id)
            .ok_or_else(|| AppError::NotFound(format!("artifact safety report {artifact_id}")))?;
        let _ = fs::remove_file(&report.path);
        report.status = ArtifactSafetyStatus::Purged;
        report.purged_at_ms = Some(now_ms());
        Ok(report.clone())
    }
}

impl Default for ArtifactSafetyService {
    fn default() -> Self {
        Self::new()
    }
}

pub fn scan_bytes(
    artifact_id: &str,
    path: &str,
    source_url: Option<String>,
    declared_content_type: Option<&str>,
    bytes: &[u8],
) -> ArtifactSafetyReport {
    let mut warnings = Vec::new();
    let mime_guess = guess_mime(path, declared_content_type, bytes);
    if bytes.len() > MAX_SAFE_PREVIEW_BYTES {
        warnings.push("artifact exceeds preferred safe preview size".into());
    }
    if has_executable_signature(bytes) || executable_extension(path) {
        warnings.push("artifact has executable characteristics and must not be auto-opened".into());
    }
    if archive_extension(path) {
        warnings.push("archive content requires downstream extraction review".into());
    }
    if declared_content_type.is_some_and(|declared| {
        !declared.eq_ignore_ascii_case(&mime_guess) && !declared.contains("html")
    }) {
        warnings.push("declared content type differs from local signature guess".into());
    }

    let risk_score = risk_score(&warnings, bytes.len());
    let status = if warnings.is_empty() {
        ArtifactSafetyStatus::Safe
    } else if risk_score >= 0.72 {
        ArtifactSafetyStatus::QuarantinedWarning
    } else {
        ArtifactSafetyStatus::Warning
    };

    ArtifactSafetyReport {
        artifact_id: artifact_id.to_string(),
        source_url,
        path: path.to_string(),
        content_hash: sha3_hex(bytes),
        byte_len: bytes.len(),
        mime_guess,
        status,
        risk_score,
        warnings,
        scanned_at_ms: now_ms(),
        purged_at_ms: None,
    }
}

pub fn placeholder_report_for_path(
    path: &str,
    source_url: Option<String>,
    declared_content_type: Option<&str>,
) -> ArtifactSafetyReport {
    let artifact_id = new_id("artifact_safety");
    match fs::read(path) {
        Ok(bytes) => scan_bytes(
            &artifact_id,
            path,
            source_url,
            declared_content_type,
            &bytes,
        ),
        Err(_) => ArtifactSafetyReport {
            artifact_id,
            source_url,
            path: path.to_string(),
            content_hash: sha3_hex(path.as_bytes()),
            byte_len: 0,
            mime_guess: declared_content_type
                .unwrap_or("application/octet-stream")
                .into(),
            status: ArtifactSafetyStatus::Warning,
            risk_score: 0.45,
            warnings: vec!["artifact file was unavailable for byte-level scan".into()],
            scanned_at_ms: now_ms(),
            purged_at_ms: None,
        },
    }
}

fn guess_mime(path: &str, declared_content_type: Option<&str>, bytes: &[u8]) -> String {
    if bytes.starts_with(b"<!doctype html")
        || bytes.starts_with(b"<html")
        || declared_content_type.is_some_and(|value| value.contains("html"))
    {
        "text/html".into()
    } else if bytes.starts_with(b"{") || bytes.starts_with(b"[") {
        "application/json".into()
    } else if has_executable_signature(bytes) {
        "application/x-executable".into()
    } else if path.ends_with(".zip") {
        "application/zip".into()
    } else {
        declared_content_type
            .unwrap_or("application/octet-stream")
            .to_string()
    }
}

fn has_executable_signature(bytes: &[u8]) -> bool {
    bytes.starts_with(b"MZ")
        || bytes.starts_with(&[0x7f, b'E', b'L', b'F'])
        || bytes.starts_with(&[0xcf, 0xfa, 0xed, 0xfe])
}

fn executable_extension(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    [
        ".exe", ".dll", ".msi", ".bat", ".cmd", ".scr", ".apk", ".jar",
    ]
    .iter()
    .any(|suffix| lower.ends_with(suffix))
}

fn archive_extension(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    [".zip", ".rar", ".7z", ".tar", ".gz"]
        .iter()
        .any(|suffix| lower.ends_with(suffix))
}

fn risk_score(warnings: &[String], byte_len: usize) -> f64 {
    let mut score = warnings.len() as f64 * 0.22;
    if byte_len > MAX_SAFE_PREVIEW_BYTES {
        score += 0.18;
    }
    score.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executable_bytes_are_warned_not_marked_safe() {
        let report = scan_bytes(
            "artifact_test",
            "download.exe",
            Some("https://example.com/download.exe".into()),
            Some("application/octet-stream"),
            b"MZfake",
        );

        assert!(!report.warnings.is_empty());
        assert!(matches!(
            report.status,
            ArtifactSafetyStatus::Warning | ArtifactSafetyStatus::QuarantinedWarning
        ));
    }
}
