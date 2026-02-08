fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Compile proto files from workspace root proto/ directory
    // ACTON_PROTO_DIR env var overrides the default "proto" location
    // Since we're in crates/talon-channel-sdk, we need to point to ../../proto
    let proto_dir = std::env::var("ACTON_PROTO_DIR").unwrap_or_else(|_| "../../proto".to_string());

    acton_service::build_utils::compile_protos_from_dir(&proto_dir)?;
    Ok(())
}
