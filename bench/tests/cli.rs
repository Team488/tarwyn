use std::process::Command;

#[test]
fn running_a_case_absent_from_the_catalog_fails_loudly() {
    let binary = env!("CARGO_BIN_EXE_bench");
    let output = Command::new(binary)
        .args(["run", "--case", "not_a_case", "--impl", "tarwyn-rust"])
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
        .args(["run", "--case", "get", "--impl", "ntcore"])
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
fn a_delivery_case_with_an_unwired_role_fails_rather_than_measuring_nothing() {
    let binary = env!("CARGO_BIN_EXE_bench");
    let output = Command::new(binary)
        .args([
            "run",
            "--case",
            "publish",
            "--impl",
            "ntcore",
            "--role",
            "publisher",
        ])
        .output()
        .expect("the bench binary runs");
    assert!(
        !output.status.success(),
        "ntcore has no publisher wired up for delivery"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("ntcore"),
        "the error must name it: {stderr}"
    );
    assert!(
        stderr.contains("publisher"),
        "the error must name the role: {stderr}"
    );
}
