//! Where the probes live and what versions they are.

use std::path::{Path, PathBuf};

pub(crate) struct Env {
    pub(crate) root: PathBuf,
    pub(crate) exe: PathBuf,
    pub(crate) server: PathBuf,
    /// What the server binary reports with `--version`.
    pub(crate) server_version: String,
    pub(crate) ntcore_version: String,
}

impl Env {
    /// Locate everything relative to this binary's workspace root.
    ///
    /// # Errors
    ///
    /// Returns an error when the root cannot be found, or when the server
    /// binary's version differs from this crate's.
    pub(crate) fn discover() -> anyhow::Result<Self> {
        let exe = std::env::current_exe()?;
        let root = exe
            .ancestors()
            .nth(3)
            .ok_or_else(|| anyhow::anyhow!("cannot find the workspace root from the binary"))?
            .to_path_buf();
        let ntcore_version = read_pyntcore_pin(&root)
            .and_then(|pin| pin.split_once("==").map(|(_, v)| v.to_string()))
            .unwrap_or_else(|| "unpinned".to_string());

        let server = root.join(format!(
            "target/release/tarwyn_server{}",
            std::env::consts::EXE_SUFFIX
        ));
        let server_version = server_version(&server)?;
        if server_version != env!("CARGO_PKG_VERSION") {
            return Err(anyhow::anyhow!(
                "{} is version {server_version}, this harness is {}; run \
                 `cargo build --release --workspace` first",
                server.display(),
                env!("CARGO_PKG_VERSION")
            ));
        }

        Ok(Env {
            server,
            exe,
            server_version,
            ntcore_version,
            root,
        })
    }

    /// The version to record for an implementation the probe cannot name itself.
    pub(crate) fn version_of(&self, implementation: &str) -> String {
        match implementation {
            "ntcore" => self.ntcore_version.clone(),
            _ => self.server_version.clone(),
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

/// The version `server --version` prints, its last word.
fn server_version(server: &Path) -> anyhow::Result<String> {
    let output = std::process::Command::new(server)
        .arg("--version")
        .output()
        .map_err(|error| {
            anyhow::anyhow!(
                "cannot run {}: {error}; run `cargo build --release --workspace` first",
                server.display()
            )
        })?;
    String::from_utf8_lossy(&output.stdout)
        .split_whitespace()
        .last()
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("{} printed no version", server.display()))
}

/// The `pyntcore==<version>` requirement in the probe's pyproject, if pinned.
pub(crate) fn read_pyntcore_pin(root: &Path) -> Option<String> {
    let text = std::fs::read_to_string(root.join("bench/python/pyproject.toml")).ok()?;
    text.lines()
        .find(|line| line.contains("pyntcore=="))
        .and_then(|line| line.split('"').nth(1).map(str::to_string))
}
