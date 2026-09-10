use std::process::Command;

#[test]
fn running_a_case_absent_from_the_catalog_fails_loudly() {
    let binary = env!("CARGO_BIN_EXE_bench");
    let output = Command::new(binary)
        .args([
            "run",
            "--case",
            "not_a_case",
            "--impl",
            "tarwyn-rust",
            "--role",
            "subscriber",
        ])
        .output()
        .expect("the bench binary runs");
    assert!(!output.status.success(), "an unknown case must not succeed");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not_a_case"),
        "the error must name it: {stderr}"
    );
}

#[test]
fn an_implementation_the_case_does_not_declare_is_refused() {
    let binary = env!("CARGO_BIN_EXE_bench");
    let output = Command::new(binary)
        .args([
            "run", "--case", "get", "--impl", "ntcore", "--role", "caller",
        ])
        .output()
        .expect("the bench binary runs");
    assert!(!output.status.success(), "ntcore has no round-trip plane");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ntcore"),
        "the error must name it: {stderr}"
    );
}

#[test]
fn an_implementation_a_case_does_not_declare_is_refused() {
    let binary = env!("CARGO_BIN_EXE_bench");
    let output = Command::new(binary)
        .args([
            "run",
            "--case",
            "publish",
            "--impl",
            "tarwyn",
            "--role",
            "publisher",
        ])
        .output()
        .expect("the bench binary runs");
    assert!(
        !output.status.success(),
        "tarwyn cannot be driven by a raw client, so it has no servers row"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("tarwyn"),
        "the error must name it: {stderr}"
    );
}
