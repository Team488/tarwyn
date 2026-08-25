fn main() {
    // protox is a pure-Rust protobuf compiler; using it here removes the
    // requirement for a system `protoc` binary, which otherwise makes a clean
    // build fail on any machine that doesn't already have one installed.
    let file_descriptors = protox::compile(["proto/messages.proto"], ["proto"])
        .expect("failed to compile messages.proto");
    prost_build::Config::new()
        .compile_fds(file_descriptors)
        .expect("failed to generate Rust types from messages.proto");
}
