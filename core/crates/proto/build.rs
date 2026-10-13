fn main() {
    let proto_dir = std::path::Path::new("../../../proto");
    let proto = proto_dir.join("api.proto");
    println!("cargo:rerun-if-changed={}", proto.display());
    let protoc = protoc_bin_vendored::protoc_bin_path().expect("vendored protoc");
    let mut cfg = prost_build::Config::new();
    cfg.protoc_executable(protoc);
    cfg.compile_protos(&[proto], &[proto_dir])
        .expect("compile api.proto");
}
