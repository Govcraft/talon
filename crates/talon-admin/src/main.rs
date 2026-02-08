//! Binary entry point for the Talon admin dashboard.

use acton_service::prelude::*;
use talon_admin::routes;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let routes = routes::build_routes();

    ServiceBuilder::new()
        .with_routes(routes)
        .build()
        .serve()
        .await?;

    Ok(())
}
