use acton_service::prelude::*;
use talon_gateway::{grpc_service, routes};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let routes = routes::build_routes();

    let grpc_routes = acton_service::grpc::server::GrpcServicesBuilder::new()
        .with_health()
        .with_reflection()
        .add_file_descriptor_set(grpc_service::FILE_DESCRIPTOR_SET)
        .add_service(grpc_service::create_gateway_service())
        .build(None);

    ServiceBuilder::new()
        .with_routes(routes)
        .with_grpc_services(grpc_routes)
        .build()
        .serve()
        .await?;

    Ok(())
}
