//! Local workspace index — the self-hosted stand-in for the vendor's semantic
//! index.
//!
//! The decrypted `WorkspaceDiscoveryManager` validates a path, denies blocked
//! paths, refuses workspaces that are too large, and caches them with an LRU.
//! The indexing itself happened server-side. This module keeps the same
//! *contract* (validate -> qualify -> cache -> search) but does the retrieval
//! locally with lexical scoring, which needs no backend and no embeddings.
//!
//! Ignore handling follows the SDK's documented behaviour: `.gitignore` and
//! `.augmentignore` are both respected, with `.karmxignore` added. That matters
//! because indexing `node_modules` is the documented cause of timeouts.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Directories that never belong in an index, even if not gitignored.
const ALWAYS_SKIP: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".venv",
    "__pycache__",
    ".next",
];

#[derive(Debug, Clone)]
pub struct Hit {
    pub path: PathBuf,
    pub score: i64,
    pub line: usize,
    pub snippet: String,
}

#[derive(Debug)]
pub struct WorkspaceIndex {
    root: PathBuf,
    files: Vec<PathBuf>,
    file_count: usize,
}

impl WorkspaceIndex {
    /// Walk `root`, honouring ignore files.
    ///
    /// `max_files` mirrors the original's `qualifyWorkspace(maxTrackableFileCount)`
    /// guard: exceeding it is an error rather than a silent partial index, so the
    /// caller can tell the user the workspace is too large instead of returning
    /// misleadingly thin results.
    pub fn build(root: impl AsRef<Path>, max_files: usize) -> Result<Self> {
        let root = root
            .as_ref()
            .canonicalize()
            .with_context(|| format!("workspace path not found: {}", root.as_ref().display()))?;
        if !root.is_dir() {
            anyhow::bail!("workspace path is not a directory: {}", root.display());
        }

        let mut builder = ignore::WalkBuilder::new(&root);
        builder
            .hidden(false)
            .git_ignore(true)
            .git_global(false)
            .git_exclude(true)
            .add_custom_ignore_filename(".augmentignore")
            .add_custom_ignore_filename(".karmxignore")
            .filter_entry(|e| {
                let name = e.file_name().to_string_lossy();
                !ALWAYS_SKIP.contains(&name.as_ref())
            });

        let mut files = Vec::new();
        for entry in builder.build() {
            let entry = match entry {
                Ok(e) => e,
                Err(_) => continue,
            };
            if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
                continue;
            }
            files.push(entry.into_path());
            if files.len() > max_files {
                anyhow::bail!(
                    "workspace has more than {max_files} files; add a .gitignore or .karmxignore entry to exclude large directories"
                );
            }
        }

        let file_count = files.len();
        Ok(Self {
            root,
            files,
            file_count,
        })
    }

    pub fn file_count(&self) -> usize {
        self.file_count
    }

    /// Lexical search: score files by how many query terms they contain, then
    /// return the best matching lines.
    ///
    /// Deliberately not semantic. A wrong top-k chunk is worse than no chunk,
    /// and a local lexical hit is exact and cheap.
    pub fn search(&self, query: &str, max_hits: usize) -> Vec<Hit> {
        let terms: Vec<String> = query
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|t| t.len() > 2)
            .map(|t| t.to_lowercase())
            .collect();
        if terms.is_empty() {
            return Vec::new();
        }

        let mut hits: Vec<Hit> = Vec::new();
        for path in &self.files {
            let content = match std::fs::read_to_string(path) {
                Ok(c) => c,
                Err(_) => continue, // binary or unreadable
            };
            let lower = content.to_lowercase();

            // file-level score: how many distinct terms appear at all
            let distinct = terms.iter().filter(|t| lower.contains(t.as_str())).count();
            if distinct == 0 {
                continue;
            }

            for (i, line) in content.lines().enumerate() {
                let ll = line.to_lowercase();
                let line_score: i64 = terms
                    .iter()
                    .map(|t| if ll.contains(t.as_str()) { 1 } else { 0 })
                    .sum();
                if line_score > 0 {
                    hits.push(Hit {
                        path: path.clone(),
                        // weight distinct-term coverage so files matching more
                        // of the query outrank files repeating one term
                        score: line_score + distinct as i64 * 2,
                        line: i + 1,
                        snippet: line.trim().chars().take(240).collect(),
                    });
                }
            }
        }

        hits.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.path.cmp(&b.path))
                .then(a.line.cmp(&b.line))
        });
        hits.truncate(max_hits);
        hits
    }

    /// Render hits the way a retrieval result is presented to a model.
    pub fn format_hits(&self, hits: &[Hit]) -> String {
        if hits.is_empty() {
            return "The following code sections were retrieved:\n".to_string();
        }
        let mut out = String::from("The following code sections were retrieved:\n");
        for h in hits {
            let rel = h.path.strip_prefix(&self.root).unwrap_or(&h.path);
            out.push_str(&format!(
                "\n{}:{} (score {})\n{}\n",
                rel.display(),
                h.line,
                h.score,
                h.snippet
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unique per test name: tests run in parallel, so a PID-only suffix makes
    /// them share (and clobber) one directory.
    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("karmx-ws-{}-{}", std::process::id(), name));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn indexes_and_finds_terms() {
        let d = tmp("find");
        std::fs::write(d.join("a.rs"), "fn authenticate() {}\n// unrelated\n").unwrap();
        std::fs::write(d.join("b.rs"), "fn other() {}\n").unwrap();
        let idx = WorkspaceIndex::build(&d, 1000).unwrap();
        assert_eq!(idx.file_count(), 2);
        let hits = idx.search("authenticate", 10);
        assert_eq!(hits.len(), 1, "only a.rs mentions it");
        assert!(hits[0].path.ends_with("a.rs"));
        assert_eq!(hits[0].line, 1);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn missing_directory_is_an_error_not_empty_index() {
        assert!(WorkspaceIndex::build("/definitely/not/here", 100).is_err());
    }

    #[test]
    fn too_many_files_is_rejected() {
        let d = tmp("toomany");
        for i in 0..12 {
            std::fs::write(d.join(format!("f{i}.txt")), "x").unwrap();
        }
        let err = WorkspaceIndex::build(&d, 5).unwrap_err().to_string();
        assert!(err.contains("more than 5 files"), "got: {err}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn gitignored_paths_are_skipped() {
        let d = tmp("ignored");
        std::fs::create_dir_all(d.join("node_modules")).unwrap();
        std::fs::write(d.join("node_modules/big.js"), "authenticate").unwrap();
        std::fs::write(d.join("keep.rs"), "authenticate").unwrap();
        let idx = WorkspaceIndex::build(&d, 1000).unwrap();
        let hits = idx.search("authenticate", 10);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].path.ends_with("keep.rs"));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn empty_query_returns_nothing() {
        let d = tmp("emptyq");
        std::fs::write(d.join("a.rs"), "content").unwrap();
        let idx = WorkspaceIndex::build(&d, 100).unwrap();
        assert!(idx.search("a b", 10).is_empty(), "terms too short");
        let _ = std::fs::remove_dir_all(&d);
    }
}
