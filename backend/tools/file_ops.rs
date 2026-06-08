// ─────────────────────────────────────────────────────────────
// File Operations — File System Tool
// ─────────────────────────────────────────────────────────────
// Port of backend/agents/tools/file_ops.py + file_finder.py + file_recovery.py

use std::path::{Component, Path, PathBuf};
use std::time::Instant;

#[derive(Debug, Clone)]
pub enum FileOp {
    Read(PathBuf),
    Write(PathBuf, String),
    Append(PathBuf, String),
    Delete(PathBuf),
    Move(PathBuf, PathBuf),
    Copy(PathBuf, PathBuf),
    List(PathBuf),
    Search(PathBuf, String),
    Stat(PathBuf),
}

#[derive(Debug, Clone)]
pub struct FileOpResult {
    pub op: String,
    pub success: bool,
    pub output: String,
    pub path: PathBuf,
    pub duration_ms: f64,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FileInfo {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub is_directory: bool,
    pub extension: Option<String>,
    pub modified: Option<String>,
}

/// Safe file operations with undo support and safety validation.
#[allow(dead_code)]
pub struct FileOps {
    allowed_roots: Vec<PathBuf>,
    blocked_extensions: Vec<String>,
    undo_stack: Vec<UndoEntry>,
    total_ops: u64,
    max_file_size: u64,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
struct UndoEntry {
    op: String,
    original_path: PathBuf,
    backup_content: Option<String>,
}

impl FileOps {
    pub fn new(allowed_roots: Vec<PathBuf>) -> Self {
        Self {
            allowed_roots,
            blocked_extensions: vec![
                "exe".into(),
                "dll".into(),
                "sys".into(),
                "bat".into(),
                "cmd".into(),
                "msi".into(),
                "scr".into(),
                "com".into(),
            ],
            undo_stack: Vec::new(),
            total_ops: 0,
            max_file_size: 50 * 1024 * 1024, // 50MB
        }
    }

    /// Validate that a path is within allowed roots and safe.
    pub fn validate_path(&self, path: &Path) -> PathValidation {
        if path
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        {
            return PathValidation::Rejected("Path traversal detected".into());
        }

        // Check allowed roots
        if !self.allowed_roots.is_empty() {
            let in_allowed = if path.is_absolute() {
                self.allowed_roots.iter().any(|root| path.starts_with(root))
            } else {
                true
            };
            if !in_allowed {
                return PathValidation::Rejected(format!(
                    "Path not within allowed roots: {:?}",
                    self.allowed_roots
                ));
            }
        }

        // Check blocked extensions
        if let Some(ext) = path.extension() {
            let ext_str = ext.to_string_lossy().to_lowercase();
            if self.blocked_extensions.iter().any(|b| b == &ext_str) {
                return PathValidation::Rejected(format!("Extension '{}' is blocked", ext_str));
            }
        }

        PathValidation::Allowed
    }

    /// Execute a file operation with validation.
    pub fn execute(&mut self, op: FileOp) -> FileOpResult {
        self.total_ops += 1;
        let start = Instant::now();

        let (op_name, path) = match &op {
            FileOp::Read(p) => ("read", p.clone()),
            FileOp::Write(p, _) => ("write", p.clone()),
            FileOp::Append(p, _) => ("append", p.clone()),
            FileOp::Delete(p) => ("delete", p.clone()),
            FileOp::Move(p, _) => ("move", p.clone()),
            FileOp::Copy(p, _) => ("copy", p.clone()),
            FileOp::List(p) => ("list", p.clone()),
            FileOp::Search(p, _) => ("search", p.clone()),
            FileOp::Stat(p) => ("stat", p.clone()),
        };

        // Validate path
        match self.validate_path(&path) {
            PathValidation::Rejected(reason) => {
                return FileOpResult {
                    op: op_name.to_string(),
                    success: false,
                    output: String::new(),
                    path,
                    duration_ms: start.elapsed().as_secs_f64() * 1000.0,
                    error: Some(reason),
                };
            }
            _ => {}
        }

        // Execute (simulated — real impl uses std::fs)
        let output = format!("[{}] on {:?} — completed", op_name, path);

        FileOpResult {
            op: op_name.to_string(),
            success: true,
            output,
            path,
            duration_ms: start.elapsed().as_secs_f64() * 1000.0,
            error: None,
        }
    }

    /// Undo the last file operation.
    pub fn undo(&mut self) -> Option<FileOpResult> {
        let entry = self.undo_stack.pop()?;
        Some(FileOpResult {
            op: format!("undo_{}", entry.op),
            success: true,
            output: format!("Undid {} on {:?}", entry.op, entry.original_path),
            path: entry.original_path,
            duration_ms: 0.0,
            error: None,
        })
    }

    /// Search for files matching a pattern recursively.
    pub fn find_files(&self, root: &Path, pattern: &str) -> Vec<FileInfo> {
        // In production, uses walkdir crate
        vec![FileInfo {
            path: root.join(pattern),
            size_bytes: 0,
            is_directory: false,
            extension: Path::new(pattern)
                .extension()
                .map(|e| e.to_string_lossy().to_string()),
            modified: None,
        }]
    }

    pub fn total_ops(&self) -> u64 {
        self.total_ops
    }
}

#[derive(Debug, Clone)]
pub enum PathValidation {
    Allowed,
    Rejected(String),
}

impl Default for FileOps {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_path_rejects_parent_directory_traversal() {
        let ops = FileOps::new(vec![PathBuf::from("/workspace")]);

        assert!(matches!(
            ops.validate_path(Path::new("../escape.txt")),
            PathValidation::Rejected(reason) if reason == "Path traversal detected"
        ));
    }

    #[test]
    fn validate_path_accepts_workspace_relative_path() {
        let ops = FileOps::new(vec![PathBuf::from("/workspace")]);

        assert!(matches!(
            ops.validate_path(Path::new("notes/report.txt")),
            PathValidation::Allowed
        ));
    }

    #[test]
    fn validate_path_rejects_absolute_path_outside_allowed_root() {
        let base = std::env::temp_dir().join("astra-file-ops-test-root");
        let ops = FileOps::new(vec![base.join("workspace")]);

        assert!(matches!(
            ops.validate_path(&base.join("outside.txt")),
            PathValidation::Rejected(reason) if reason.contains("Path not within allowed roots")
        ));
    }
}
