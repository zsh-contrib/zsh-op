use anyhow::{Context, Result};
use std::cell::OnceCell;
use std::fs::{DirBuilder, OpenOptions, Permissions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use crate::log::warn;

/// RuntimeDir is the private directory file secrets are written to.
pub struct RuntimeDir {
    /// Directory given by the caller.
    root: Option<PathBuf>,
    /// Temporary directory created on first use when no directory was given.
    created: OnceCell<PathBuf>,
}

impl RuntimeDir {
    /// Creates a runtime directory rooted at `root`, or at a new temporary directory.
    pub fn new(root: Option<PathBuf>) -> Self {
        Self {
            root,
            created: OnceCell::new(),
        }
    }

    /// Returns the root directory, creating a temporary one if needed.
    fn root(&self) -> Result<&Path> {
        if let Some(root) = &self.root {
            return Ok(root);
        }
        if let Some(created) = self.created.get() {
            return Ok(created);
        }

        let dir = tempfile::Builder::new()
            .prefix("zsh-op.")
            .tempdir()
            .context("failed to create file secret runtime directory")?
            .keep();
        warn(format!(
            "file secrets are written to {}; remove it when done",
            dir.display()
        ));
        Ok(self.created.get_or_init(|| dir))
    }

    /// Writes secret `name` of `profile` to a private file and returns its path.
    pub fn write(&self, profile: &str, name: &str, value: &str) -> Result<PathBuf> {
        let root = self.root()?;
        let dir = root.join("files").join(profile);
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)
            .with_context(|| format!("failed to create {}", dir.display()))?;
        for path in [root, &root.join("files"), &dir] {
            std::fs::set_permissions(path, Permissions::from_mode(0o700))?;
        }

        let path = dir.join(name);
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)
            .with_context(|| format!("failed to write {}", path.display()))?;
        file.set_permissions(Permissions::from_mode(0o600))?;
        file.write_all(value.as_bytes())?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mode(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn write_creates_private_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("runtime");
        let runtime = RuntimeDir::new(Some(root.clone()));

        let path = runtime.write("personal", "GCP_CREDENTIALS", "{}").unwrap();

        assert_eq!(path, root.join("files/personal/GCP_CREDENTIALS"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{}");
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(path.parent().unwrap()), 0o700);
        assert_eq!(mode(&root), 0o700);
    }

    #[test]
    fn write_replaces_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let runtime = RuntimeDir::new(Some(dir.path().to_path_buf()));

        runtime.write("personal", "A", "old value").unwrap();
        let path = runtime.write("personal", "A", "new").unwrap();

        assert_eq!(std::fs::read_to_string(path).unwrap(), "new");
    }
}
