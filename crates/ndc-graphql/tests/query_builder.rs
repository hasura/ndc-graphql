use common::capabilities_response::capabilities_response;
use common::config::ServerConfig;
use common::{config::config_file::ServerConfigFile, schema_response::schema_response};
use insta::{assert_json_snapshot, assert_snapshot, assert_yaml_snapshot, glob};
use ndc_graphql::{
    connector::setup::GraphQLConnectorSetup,
    query_builder::{build_mutation_document, build_query_document},
};
use ndc_sdk::models;
use schemars::schema_for;
use std::{collections::HashMap, fs, path::PathBuf};

#[tokio::test]
#[ignore]
async fn update_json_schema() {
    for config in ["config-1", "config-2", "config-3"] {
        fs::write(
            format!("./tests/{config}/queries/_query_request.schema.json"),
            serde_json::to_string_pretty(&schema_for!(models::QueryRequest))
                .expect("Should serialize schema to json"),
        )
        .expect("Should be able to write out schema file");
        fs::write(
            format!("./tests/{config}/mutations/_mutation_request.schema.json"),
            serde_json::to_string_pretty(&schema_for!(models::MutationRequest))
                .expect("Should serialize schema to json"),
        )
        .expect("Should be able to write out schema file");
        fs::write(
            format!("./tests/{config}/configuration/configuration.schema.json"),
            serde_json::to_string_pretty(&schema_for!(ServerConfigFile))
                .expect("Should serialize schema to json"),
        )
        .expect("Should be able to write out schema file");
    }
}

async fn read_configuration(config: &str) -> ServerConfig {
    let configuration_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(config)
        .join("configuration");
    let env = HashMap::from_iter(vec![
        ("GRAPHQL_ENDPOINT".to_owned(), "".to_owned()),
        ("GRAPHQL_ENDPOINT_SECRET".to_owned(), "".to_owned()),
    ]);
    GraphQLConnectorSetup::new(env)
        .read_configuration(configuration_dir)
        .await
        .expect("Should sucessfully read configuration")
}

// We use insta for snapshot testing
// Install it with `cargo install cargo-insta`
// The usual workflow is to run `cargo insta test`, then `cargo insta review`
// For more info, see insta docs: https://insta.rs/

#[tokio::test]
async fn test_build_graphql_query() {
    for config in ["config-1", "config-2", "config-3"] {
        let configuration = read_configuration(config).await;

        glob!(format!("./{config}/queries"), "*.request.json", |path| {
            let request = fs::read_to_string(path).expect("Should be able to read file");
            let request: models::QueryRequest =
                serde_json::from_str(&request).expect("Should be valid request json");
            let operation = build_query_document(&request, &configuration)
                .expect("Should sucessfully build query document");

            assert_snapshot!("Query String", operation.query);
            assert_json_snapshot!("Variables", operation.variables);
            assert_yaml_snapshot!("Headers", operation.headers);
        });
    }
}

#[tokio::test]
async fn test_build_graphql_mutation() {
    for config in ["config-1", "config-2", "config-3"] {
        let configuration = read_configuration(config).await;

        glob!(format!("./{config}/mutations"), "*.request.json", |path| {
            let request = fs::read_to_string(path).expect("Should be able to read file");
            let request: models::MutationRequest =
                serde_json::from_str(&request).expect("Should be valid request json");
            let operation = build_mutation_document(&request, &configuration)
                .expect("Should sucessfully build query document");

            assert_snapshot!("Query String", operation.query);
            assert_json_snapshot!("Variables", operation.variables);
            assert_yaml_snapshot!("Headers", operation.headers);
        });
    }
}

#[tokio::test]
async fn test_generated_schema() {
    for config in ["config-1", "config-2", "config-3"] {
        let configuration = read_configuration(config).await;
        let schema = schema_response(
            &configuration.schema,
            &configuration.request,
            &configuration.response,
        );
        assert_yaml_snapshot!(format!("{config} NDC Schema"), schema);
    }
}

#[test]
fn test_capabilities() {
    assert_yaml_snapshot!("Capabilities", capabilities_response())
}
