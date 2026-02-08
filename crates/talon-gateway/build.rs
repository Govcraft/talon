fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_dir = std::env::var("ACTON_PROTO_DIR").unwrap_or_else(|_| "../../proto".to_string());
    acton_service::build_utils::compile_protos_from_dir(&proto_dir)?;
    Ok(())
}
