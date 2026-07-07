fn main() {
    tonic_build::configure()
        .out_dir("src/proto")
        .compile_protos(
            &[
                "proto/bidder_service.proto",
                "proto/retriever_service.proto",
                "proto/validator_service.proto",
            ],
            &["proto"],
        )
        .unwrap();
}
