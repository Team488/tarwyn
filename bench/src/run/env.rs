//! Where the probes live and what versions they are.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub(crate) struct Env {
    pub(crate) root: PathBuf,
    pub(crate) exe: PathBuf,
    pub(crate) server: PathBuf,
    pub(crate) pyntcore: String,
    pub(crate) ntcore_version: String,
    pub(crate) java_classpath: Option<String>,
    pub(crate) tarwyn_jar: Option<String>,
    pub(crate) tarwyn_version: String,
}

impl Env {
    /// Locate everything relative to this binary's workspace root.
    ///
    /// The Java classpath is resolved through gradle on every run rather than
    /// read from a cached file: gradle is incremental, and the alternative is
    /// benchmarking a stale class file against a current one.
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

        let gradlew = if cfg!(windows) {
            "gradlew.bat"
        } else {
            "gradlew"
        };
        let _ = Command::new(root.join(gradlew))
            .arg("-q")
            .arg("benchEnv")
            .current_dir(&root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let env_file = std::fs::read_to_string(root.join("build/bench-env.sh")).unwrap_or_default();
        let field = |key: &str| {
            env_file.lines().find_map(|line| {
                line.strip_prefix(key)?
                    .strip_prefix("='")?
                    .strip_suffix('\'')
                    .map(str::to_string)
            })
        };

        Ok(Env {
            server: root.join(format!(
                "target/release/tarwyn_server{}",
                std::env::consts::EXE_SUFFIX
            )),
            exe,
            pyntcore,
            ntcore_version,
            java_classpath: field("BENCH_CP"),
            tarwyn_jar: field("BENCH_TARWYN_JAR"),
            tarwyn_version: field("BENCH_TARWYN_VERSION")
                .map(|v| v.trim_start_matches('v').to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            root,
        })
    }

    /// The version to record for an implementation the probe cannot name itself.
    pub(crate) fn version_of(&self, implementation: &str) -> String {
        match implementation {
            "ntcore" => self.ntcore_version.clone(),
            "tarwyn" => self.tarwyn_version.clone(),
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
