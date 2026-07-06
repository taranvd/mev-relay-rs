fn main() {
    tonic_build::configure()
        .out_dir("src/proto")
        .compile_protos(&["proto/bidder_service.proto"], &["proto"])
        .unwrap();
}
