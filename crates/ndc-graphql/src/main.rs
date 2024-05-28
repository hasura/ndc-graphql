use ndc_graphql::connector::GraphQLConnector;
use ndc_sdk::default_main::default_main;

use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    default_main::<GraphQLConnector>().await
}
