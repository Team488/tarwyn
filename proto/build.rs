fn main() {
    let file_descriptors = protox::compile(["proto/messages.proto"], ["proto"])
        .expect("failed to compile messages.proto");
    prost_build::Config::new()
        .boxed(".protobuf.CompareAndSetCommand.expected")
        .boxed(".protobuf.CompareAndSetCommand.value")
        .boxed(".protobuf.ReplyCompareAndSetCommand.current")
        .compile_fds(file_descriptors)
        .expect("failed to generate Rust types from messages.proto");
}
