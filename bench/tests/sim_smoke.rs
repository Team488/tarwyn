use std::process::Command;

/// The tiny profile end to end against tarwyn at 1x: every role runs, and the
/// report gains a passing row. Needs `cargo build --release --workspace`.
#[test]
#[ignore = "needs the release server binary"]
fn sim_smoke() {
    let binary = env!("CARGO_BIN_EXE_bench");
    let dir = std::env::temp_dir().join(format!("bench-sim-smoke-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let profile = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/tiny-profile.json"
    );
    let output = Command::new(binary)
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/.."))
        .args([
            "sim",
            "--profile",
            profile,
            "--axes",
            "rate",
            "--impls",
            "tarwyn",
            "--budgets",
            "none",
            "--reps",
            "1",
            "--warmup",
            "1",
            "--seconds",
            "2",
            "--max-multiplier",
            "1",
            "--probes",
            "1",
        ])
        .arg("--logs")
        .arg(dir.join("logs"))
        .arg("--json")
        .arg(dir.join("results.json"))
        .arg("--markdown")
        .arg(dir.join("RESULTS.md"))
        .output()
        .expect("the bench binary runs");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    let markdown = std::fs::read_to_string(dir.join("RESULTS.md")).unwrap();
    assert!(markdown.contains("|1x|tarwyn|"), "{markdown}");
    assert!(markdown.contains("pass (1 of 1)"), "{markdown}");
    std::fs::remove_dir_all(&dir).ok();
}
