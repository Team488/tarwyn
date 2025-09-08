fn main() {
    #[cfg(target_os = "windows")]
    println!("cargo:rustc-link-lib=advapi32");
    prost_build::compile_protos(&["../proto/messages.proto"], &["../proto"]).unwrap();
}
