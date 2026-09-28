use std::env;
use std::path::PathBuf;

fn main() {
    let crate_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"));
    let header = crate_dir.join("include/tarwyn.h");
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=cbindgen.toml");
    cbindgen::generate(&crate_dir)
        .expect("the C ABI must render as a header")
        .write_to_file(header);
}
