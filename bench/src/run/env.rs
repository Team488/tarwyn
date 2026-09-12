//! Where the probes live and what versions they are.

use std::io;
use std::path::{Path, PathBuf};

pub(crate) struct Env {
    pub(crate) root: PathBuf,
    pub(crate) exe: PathBuf,
    pub(crate) server: PathBuf,
    pub(crate) pyntcore: String,
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
        let pyntcore = read_pyntcore_pin(&root);
        let ntcore_version = pyntcore
            .split_once("==")
            .map(|(_, v)| v.to_string())
            .unwrap_or_else(|| "unpinned".to_string());

        Ok(Env {
            server: root.join(format!(
                "target/release/tarwyn_server{}",
                std::env::consts::EXE_SUFFIX
            )),
            exe,
            pyntcore,
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

    pub(crate) fn python_probe(&self) -> PathBuf {
        self.root.join("bench/python/ntcore_probe.py")
    }
}

pub(crate) fn read_pyntcore_pin(root: &Path) -> String {
    std::fs::read_to_string(root.join("bindings/pyproject.toml"))
        .ok()
        .and_then(|text| {
            text.lines()
                .find(|line| line.contains("pyntcore=="))
                .and_then(|line| line.split('"').nth(1).map(str::to_string))
        })
        .unwrap_or_else(|| "pyntcore".to_string())
}
