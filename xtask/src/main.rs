//! `cargo xtask <command>`: the repository's automation, in Rust so it runs
//! the same on every platform CI builds for.

#![forbid(unsafe_code)]

use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

#[derive(Parser, Debug)]
#[command(about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Print the version the workspace is at, from the root Cargo.toml.
    Version,
    /// Zip the release artifacts a target's release build produced:
    /// `tarwyn-<platform>.zip` with the server and `tarwyn-cpp-<platform>.zip`
    /// with the C++ client's headers and library, plus the import library on
    /// Windows, which MSVC links against.
    Package {
        /// The cargo target triple the release build used.
        target: String,
        /// The platform name the archives carry.
        platform: String,
        /// Where to write the archives.
        #[arg(long, default_value = "release")]
        out: PathBuf,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Version => println!("{}", version()?),
        Command::Package {
            target,
            platform,
            out,
        } => package(&target, &platform, &out)?,
    }
    Ok(())
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask lives one level under the workspace root")
        .to_path_buf()
}

fn version() -> Result<String> {
    let manifest = fs::read_to_string(root().join("Cargo.toml"))?;
    manifest
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once('=')?;
            (key.trim() == "version").then(|| value.trim().trim_matches('"').to_string())
        })
        .context("the root Cargo.toml has no [workspace.package] version")
}

fn package(target: &str, platform: &str, out: &Path) -> Result<()> {
    let root = root();
    let build = root.join("target").join(target).join("release");
    if !build.is_dir() {
        bail!("no release build at {}", build.display());
    }
    let exe = if target.contains("windows") {
        ".exe"
    } else {
        ""
    };
    let library = if target.contains("windows") {
        "tarwyn.dll".to_string()
    } else if target.contains("apple") {
        "libtarwyn.dylib".to_string()
    } else {
        "libtarwyn.so".to_string()
    };
    let c_include = root.join("bindings/c/include");
    let cpp_include = root.join("bindings/cpp/include");
    fs::create_dir_all(out)?;

    let server = out.join(format!("tarwyn-{platform}.zip"));
    archive(
        &server,
        &[(
            format!("tarwyn_server{exe}"),
            build.join(format!("tarwyn_server{exe}")),
        )],
    )?;
    let mut cpp_entries = vec![
        ("include/tarwyn.h".to_string(), c_include.join("tarwyn.h")),
        (
            "include/tarwyn.hpp".to_string(),
            cpp_include.join("tarwyn.hpp"),
        ),
        (format!("lib/{library}"), build.join(&library)),
    ];
    if target.contains("windows") {
        cpp_entries.push((
            "lib/tarwyn.dll.lib".to_string(),
            build.join("tarwyn.dll.lib"),
        ));
    }
    let cpp = out.join(format!("tarwyn-cpp-{platform}.zip"));
    archive(&cpp, &cpp_entries)?;
    Ok(())
}

fn archive(path: &Path, entries: &[(String, PathBuf)]) -> Result<()> {
    let mut zip = ZipWriter::new(File::create(path)?);
    for (name, source) in entries {
        let bytes = fs::read(source).with_context(|| format!("reading {}", source.display()))?;
        let executable = !name.contains('.') || name.ends_with(".so") || name.ends_with(".dylib");
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .unix_permissions(if executable { 0o755 } else { 0o644 });
        zip.start_file(name, options)?;
        zip.write_all(&bytes)?;
    }
    zip.finish()?;
    println!("wrote {}", path.display());
    Ok(())
}
