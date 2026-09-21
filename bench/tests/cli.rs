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
            "tarwyn",
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
fn an_implementation_that_is_not_cataloged_is_refused() {
    let binary = env!("CARGO_BIN_EXE_bench");
    let output = Command::new(binary)
        .args([
            "run",
            "--case",
            "publish_client",
            "--impl",
            "reference",
            "--role",
            "publisher",
        ])
        .output()
        .expect("the bench binary runs");
    assert!(
        !output.status.success(),
        "no implementation called reference is cataloged"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("reference"),
        "the error must name it: {stderr}"
    );
}

#[test]
fn a_role_no_probe_implements_is_refused() {
    let binary = env!("CARGO_BIN_EXE_bench");
    let output = Command::new(binary)
        .args([
            "run", "--case", "publish", "--impl", "ntcore", "--role", "caller",
        ])
        .output()
        .expect("the bench binary runs");
    assert!(
        !output.status.success(),
        "a delivery case has a publisher and a subscriber, nothing else"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("caller"),
        "the error must name it: {stderr}"
    );
}

/// Samples due 2 ms apart, each received 40 us after it was due, plus one late
/// sample: the row must charge the lateness to the sample that waited. The
/// histogram keeps three significant figures, so a percentile lands in its
/// bucket rather than on the nose.
#[test]
fn a_foreign_harnesses_samples_become_the_same_row_as_a_native_one() {
    let binary = env!("CARGO_BIN_EXE_bench");
    let samples = std::env::temp_dir().join("bench_row_samples.tsv");
    let mut text = String::from("subscribed on 127.0.0.1:48820, waiting for 4 samples...\n");
    for seq in 0..4u64 {
        let due = 1_000_000_000 + seq * 2_000_000;
        let late = if seq == 3 { 1_000_000 } else { 0 };
        text.push_str(&format!("S\t{seq}\t{due}\t{}\n", due + 40_000 + late));
    }
    std::fs::write(&samples, text).expect("the samples file is written");

    let output = Command::new(binary)
        .env("BENCH_WARMUP", "0")
        .args([
            "row",
            "--samples",
            samples.to_str().unwrap(),
            "--case",
            "publish_client",
            "--impl",
            "ntcore",
            "--payload",
            "96",
            "--version",
            "2027.0.0",
        ])
        .output()
        .expect("the bench binary runs");
    let _ = std::fs::remove_file(&samples);

    assert!(
        output.status.success(),
        "a sample file must reduce to a row"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let row = stdout
        .lines()
        .find(|line| line.starts_with("ROW"))
        .expect("a ROW line is printed");
    let fields: Vec<&str> = row.split('\t').collect();
    assert_eq!(fields.len(), 16, "the row schema is fixed: {row}");
    assert_eq!(fields[1], "publish_client");
    assert_eq!(fields[2], "ntcore");
    assert_eq!(fields[3], "2027.0.0");
    assert_eq!(fields[4], "96");
    let us = |field: &str| field.parse::<f64>().expect("a percentile is a number");
    assert!(
        (40.0..40.1).contains(&us(fields[5])),
        "the median is due-stamped, in us: {row}"
    );
    assert!(
        (1040.0..1041.0).contains(&us(fields[12])),
        "the late sample carries its own wait: {row}"
    );
    assert_eq!(fields[14], "4", "every sample after warmup is counted");
}

#[test]
fn a_sample_file_with_nothing_in_it_fails_rather_than_reporting_zero() {
    let binary = env!("CARGO_BIN_EXE_bench");
    let samples = std::env::temp_dir().join("bench_row_empty.tsv");
    std::fs::write(&samples, "waiting for 4 samples...\n").expect("written");
    let output = Command::new(binary)
        .args([
            "row",
            "--samples",
            samples.to_str().unwrap(),
            "--case",
            "publish_client",
            "--impl",
            "ntcore",
            "--payload",
            "96",
            "--version",
            "2027.0.0",
        ])
        .output()
        .expect("the bench binary runs");
    let _ = std::fs::remove_file(&samples);
    assert!(!output.status.success(), "an empty run is not a fast one");
}

/// The middle sample arrives 12 us before its send was due, which is only
/// possible if the two processes are reading different clocks.
#[test]
fn a_sample_received_before_it_was_due_is_refused() {
    let binary = env!("CARGO_BIN_EXE_bench");
    let samples = std::env::temp_dir().join("bench_row_negative.tsv");
    let mut text = String::new();
    for seq in 0..4u64 {
        let due = 1_000_000_000 + seq * 2_000_000;
        let received = if seq == 2 { due - 12_000 } else { due + 40_000 };
        text.push_str(&format!("S\t{seq}\t{due}\t{received}\n"));
    }
    std::fs::write(&samples, text).expect("written");
    let output = Command::new(binary)
        .env("BENCH_WARMUP", "0")
        .args([
            "row",
            "--samples",
            samples.to_str().unwrap(),
            "--case",
            "publish_client",
            "--impl",
            "ntcore",
            "--payload",
            "96",
            "--version",
            "2027.0.0",
        ])
        .output()
        .expect("the bench binary runs");
    let _ = std::fs::remove_file(&samples);
    assert!(
        !output.status.success(),
        "a negative latency is not a measurement"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("clocks disagree"), "{stderr}");
}
