//! Where the probes live and what versions they are.

use std::io;
use std::path::{Path, PathBuf};

pub(crate) struct Env {
    pub(crate) root: PathBuf,
    pub(crate) exe: PathBuf,
    pub(crate) server: PathBuf,
    pub(crate) ntcore_version: String,
}

impl Env {
    /// Locate everything relative to this binary's workspace root.
    pub(crate) fn discover() -> io::Result<Self> {
        let exe = std::env::current_exe()?;
        let root = exe
            .ancestors()
            .nth(3)
            .ok_or_else(|| io::Error::other("cannot find the workspace root from the binary"))?
            .to_path_buf();
        let ntcore_version = read_pyntcore_pin(&root)
            .and_then(|pin| pin.split_once("==").map(|(_, v)| v.to_string()))
            .unwrap_or_else(|| "unpinned".to_string());

        Ok(Env {
            server: root.join(format!(
                "target/release/tarwyn_server{}",
                std::env::consts::EXE_SUFFIX
            )),
            exe,
            ntcore_version,
            root,
        })
    }

    /// The version to record for an implementation the probe cannot name itself.
    pub(crate) fn version_of(&self, implementation: &str) -> String {
        match implementation {
            "ntcore" => self.ntcore_version.clone(),
            _ => env!("CARGO_PKG_VERSION").to_string(),
        }
    }

    /// The uv project that pins `pyntcore` for the probe.
    pub(crate) fn python_project(&self) -> PathBuf {
        self.root.join("bench/python")
    }

    pub(crate) fn python_probe(&self) -> PathBuf {
        self.python_project().join("src/ntcore_probe.py")
    }
}

/// The `pyntcore==<version>` requirement in the probe's pyproject, if pinned.
pub(crate) fn read_pyntcore_pin(root: &Path) -> Option<String> {
    let text = std::fs::read_to_string(root.join("bench/python/pyproject.toml")).ok()?;
    text.lines()
        .find(|line| line.contains("pyntcore=="))
        .and_then(|line| line.split('"').nth(1).map(str::to_string))
}
