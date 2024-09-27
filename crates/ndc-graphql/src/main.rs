use ndc_graphql::connector::setup::GraphQLConnectorSetup;
use ndc_sdk::{connector::ErrorResponse, default_main::default_main};

#[tokio::main]
async fn main() -> Result<(), ErrorResponse> {
    default_main::<GraphQLConnectorSetup>().await
}
