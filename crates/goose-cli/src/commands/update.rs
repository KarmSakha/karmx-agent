use anyhow::{bail, Context, Result};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Binary name for this platform.
fn binary_name() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "karmx.exe"
    }
    #[cfg(not(target_os = "windows"))]
    {
        "karmx"
    }
}

/// Locate the karmx source checkout this binary was built from.
///
/// `CARGO_MANIFEST_DIR` is baked in at compile time and points at
/// `crates/goose-cli`, so the workspace root is two directories up.
fn repo_root() -> Result<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .context("could not locate the karmx source checkout")?;
    if !root.join("Cargo.toml").is_file() {
        bail!(
            "{} does not look like the karmx workspace; cannot rebuild from source",
            root.display()
        );
    }
    Ok(root.to_path_buf())
}

/// Refuse to build a dirty working tree unless `--canary` was passed.
///
/// A clean checkout means the rebuilt binary matches the committed source;
/// uncommitted changes produce a canary binary and must be opted into.
fn ensure_clean_tree(repo_root: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(repo_root)
        .output()
        .context("failed to run `git status`; pass --canary to build anyway")?;
    if !output.status.success() {
        bail!(
            "`git status` failed in {}; pass --canary to build anyway",
            repo_root.display()
        );
    }
    let dirty = String::from_utf8_lossy(&output.stdout);
    if !dirty.trim().is_empty() {
        bail!(
            "the karmx checkout at {} has uncommitted changes; commit them or pass --canary \
             to build a canary binary anyway",
            repo_root.display()
        );
    }
    Ok(())
}

/// Rebuild the karmx binary from source and replace the running executable.
///
/// Runs `cargo build --release --bin karmx` in the workspace this binary was
/// built from, then atomically replaces the current executable (rename-old +
/// copy, never an in-place overwrite of a running binary).
///
/// `--canary` allows building a dirty working tree; `--reconfigure` runs
/// `karmx configure` afterwards.
pub async fn update(canary: bool, reconfigure: bool) -> Result<()> {
    #[cfg(feature = "disable-update")]
    {
        bail!("Update is disabled in this build.");
    }

    #[cfg(not(feature = "disable-update"))]
    {
        let repo_root = repo_root()?;
        if !canary {
            ensure_clean_tree(&repo_root)?;
        }

        println!("Rebuilding karmx from source at {}...", repo_root.display());
        let status = Command::new("cargo")
            .args(["build", "--release", "--bin", "karmx"])
            .current_dir(&repo_root)
            .status()
            .context("failed to run `cargo build`; is the Rust toolchain installed?")?;
        if !status.success() {
            bail!("cargo build failed ({status}); not updating");
        }

        let built = repo_root.join("target").join("release").join(binary_name());
        if !built.is_file() {
            bail!(
                "expected rebuilt binary at {}; not updating",
                built.display()
            );
        }

        let current_exe =
            env::current_exe().context("Failed to determine current executable path")?;
        if built == current_exe {
            println!("karmx rebuilt successfully in place; already running the fresh binary.");
        } else {
            replace_binary(&built, &current_exe).context("Failed to replace current binary")?;
            println!("karmx updated successfully.");
        }

        if reconfigure {
            println!("Running karmx configure...");
            let status = Command::new(&current_exe)
                .arg("configure")
                .status()
                .context("Failed to run karmx configure")?;
            if !status.success() {
                eprintln!("Warning: karmx configure exited with {status}");
            }
        }

        Ok(())
    }
}

/// Replace the current binary with the newly built one.
///
/// On Windows we must rename the running exe (Windows allows rename but not
/// delete/overwrite of a locked file) then copy the new file in.
///
/// On Unix we rename the running binary aside first — copying over a running
/// executable in place is ETXTBSY on Linux and SIGKILLs the process on macOS
/// ("Code Signature Invalid"), so the destination is always unlinked/renamed
/// before the new file lands.
fn replace_binary(new_binary: &Path, current_exe: &Path) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        let old_exe = current_exe.with_extension("exe.old");

        // Clean up leftover from a previous update
        if old_exe.exists() {
            fs::remove_file(&old_exe).with_context(|| {
                format!(
                    "Failed to remove old backup {}. Is another karmx process running?",
                    old_exe.display()
                )
            })?;
        }

        // Rename the running binary out of the way
        fs::rename(current_exe, &old_exe).with_context(|| {
            format!("Failed to rename running binary to {}.", old_exe.display())
        })?;

        // Copy the new binary into place
        fs::copy(new_binary, current_exe).with_context(|| {
            // Try to restore the old binary
            let _ = fs::rename(&old_exe, current_exe);
            format!("Failed to copy new binary to {}", current_exe.display())
        })?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        let old_exe = current_exe.with_extension("old");

        // Rename current binary aside first: never overwrite a running
        // executable in place (ETXTBSY on Linux, SIGKILL on macOS).
        if current_exe.exists() {
            fs::rename(current_exe, &old_exe).with_context(|| {
                format!("Failed to rename {} before update", current_exe.display())
            })?;
        }

        if let Err(e) = fs::copy(new_binary, current_exe) {
            // Restore old binary if copy fails
            let _ = fs::rename(&old_exe, current_exe);
            return Err(e).with_context(|| {
                format!("Failed to copy new binary to {}", current_exe.display())
            });
        }

        // Delete the old backup binary
        let _ = fs::remove_file(&old_exe);

        // Ensure the binary is executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(current_exe)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(current_exe, perms)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_binary_name() {
        let name = binary_name();
        #[cfg(target_os = "windows")]
        assert_eq!(name, "karmx.exe");
        #[cfg(not(target_os = "windows"))]
        assert_eq!(name, "karmx");
    }

    #[test]
    fn test_repo_root_is_workspace() {
        let root = repo_root().unwrap();
        assert!(root.join("Cargo.toml").is_file());
        assert!(root.join("crates").is_dir());
    }

    #[test]
    fn test_replace_binary_basic() {
        let tmp = tempdir().unwrap();
        let new_bin = tmp.path().join("new_karmx");
        let current = tmp.path().join("current_karmx");

        fs::write(&new_bin, b"new version").unwrap();
        fs::write(&current, b"old version").unwrap();

        replace_binary(&new_bin, &current).unwrap();

        let content = fs::read_to_string(&current).unwrap();
        assert_eq!(content, "new version");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_replace_binary_windows_rename_away() {
        let tmp = tempdir().unwrap();
        let current = tmp.path().join("karmx.exe");
        let new_bin = tmp.path().join("new_karmx.exe");

        fs::write(&current, b"old version").unwrap();
        fs::write(&new_bin, b"new version").unwrap();

        replace_binary(&new_bin, &current).unwrap();

        let content = fs::read_to_string(&current).unwrap();
        assert_eq!(content, "new version");

        let old = current.with_extension("exe.old");
        assert!(old.exists());
        let old_content = fs::read_to_string(&old).unwrap();
        assert_eq!(old_content, "old version");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn test_replace_binary_windows_cleanup_old() {
        let tmp = tempdir().unwrap();
        let current = tmp.path().join("karmx.exe");
        let old = current.with_extension("exe.old");
        let new_bin = tmp.path().join("new_karmx.exe");

        fs::write(&current, b"version 2").unwrap();
        fs::write(&old, b"version 1").unwrap();
        fs::write(&new_bin, b"version 3").unwrap();

        replace_binary(&new_bin, &current).unwrap();

        let content = fs::read_to_string(&current).unwrap();
        assert_eq!(content, "version 3");

        let old_content = fs::read_to_string(&old).unwrap();
        assert_eq!(old_content, "version 2");
    }
}
