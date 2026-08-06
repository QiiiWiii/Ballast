fn main() -> Result<(), Box<dyn std::error::Error>> {
    let protoc = protoc_bin_vendored::protoc_bin_path()?;
    // Build scripts run before application threads exist. Setting PROTOC here
    // keeps local and container builds independent from a system installation.
    unsafe {
        std::env::set_var("PROTOC", protoc);
    }

    tonic_build::configure()
        .build_client(true)
        .build_server(false)
        .compile_protos(
            &[
                "../../proto/exchange_gateway.proto",
                "../../proto/private_gateway.proto",
            ],
            &["../../proto"],
        )?;

    println!("cargo:rerun-if-changed=../../proto/exchange_gateway.proto");
    println!("cargo:rerun-if-changed=../../proto/private_gateway.proto");
    Ok(())
}
