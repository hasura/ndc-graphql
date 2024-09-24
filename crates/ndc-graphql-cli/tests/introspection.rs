use insta::assert_snapshot;
use ndc_graphql_cli::graphql::{introspection::Introspection, schema_from_introspection};
use std::{error::Error, path::PathBuf};
use tokio::fs;

#[tokio::test]
async fn generate_schema_from_introspection() -> Result<(), Box<dyn Error>> {
    let introspection_response_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("introspection.json");
    let introspection_file = fs::read_to_string(introspection_response_path).await?;
    let introspection_response: graphql_client::Response<Introspection> =
        serde_json::from_str(&introspection_file)?;
    let introspection_data = introspection_response
        .data
        .expect("introspection test file should have data");
    let graphql_sdl = schema_from_introspection(introspection_data);
    assert_snapshot!(graphql_sdl.to_string());

    Ok(())
}
