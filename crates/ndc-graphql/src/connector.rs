use self::state::ServerState;
use crate::query_builder::{build_mutation_document, build_query_document};
use async_trait::async_trait;
use common::{
    client::{execute_graphql, GraphQLRequest},
    config::ServerConfig,
};
use indexmap::IndexMap;
use ndc_sdk::{
    connector::{
        Connector, ExplainError, FetchMetricsError, HealthError, MutationError, QueryError,
        SchemaError,
    },
    json_response::JsonResponse,
    models::{
        self, CapabilitiesResponse, LeafCapability, MutationOperationResults, RowFieldValue, RowSet,
    },
};
use schema::schema_response;
use std::{collections::BTreeMap, mem};
use tracing::Instrument;
mod schema;
pub mod setup;
mod state;

#[derive(Debug, Default, Clone)]
pub struct GraphQLConnector;

#[async_trait]
impl Connector for GraphQLConnector {
    type Configuration = ServerConfig;
    type State = ServerState;

    fn fetch_metrics(
        _configuration: &Self::Configuration,
        _state: &Self::State,
    ) -> Result<(), FetchMetricsError> {
        Ok(())
    }

    async fn health_check(
        _configuration: &Self::Configuration,
        _state: &Self::State,
    ) -> Result<(), HealthError> {
        Ok(())
    }

    async fn get_capabilities() -> JsonResponse<models::CapabilitiesResponse> {
        JsonResponse::Value(CapabilitiesResponse {
            version: "^0.1.1".to_string(),
            capabilities: models::Capabilities {
                query: models::QueryCapabilities {
                    aggregates: None,
                    variables: None,
                    explain: Some(LeafCapability {}),
                    nested_fields: models::NestedFieldCapabilities {
                        aggregates: None,
                        filter_by: None,
                        order_by: None,
                    },
                },
                mutation: models::MutationCapabilities {
                    transactional: None,
                    explain: Some(LeafCapability {}),
                },
                relationships: None,
            },
        })
    }

    async fn get_schema(
        configuration: &Self::Configuration,
    ) -> Result<JsonResponse<models::SchemaResponse>, SchemaError> {
        Ok(JsonResponse::Value(schema_response(configuration)))
    }

    async fn query_explain(
        configuration: &Self::Configuration,
        _state: &Self::State,
        request: models::QueryRequest,
    ) -> Result<JsonResponse<models::ExplainResponse>, ExplainError> {
        let operation = tracing::info_span!("Build Query Document", internal.visibility = "user")
            .in_scope(|| build_query_document(&request, configuration))?;

        let query = serde_json::to_string_pretty(&GraphQLRequest::new(
            &operation.query,
            &operation.variables,
        ))
        .map_err(ExplainError::new)?;

        let details = BTreeMap::from_iter(vec![
            ("SQL Query".to_string(), operation.query),
            ("Execution Plan".to_string(), query),
            (
                "Headers".to_string(),
                serde_json::to_string(&operation.headers).expect("should convert headers to json"),
            ),
        ]);

        Ok(JsonResponse::Value(models::ExplainResponse { details }))
    }

    async fn mutation_explain(
        configuration: &Self::Configuration,
        _state: &Self::State,
        request: models::MutationRequest,
    ) -> Result<JsonResponse<models::ExplainResponse>, ExplainError> {
        let operation =
            tracing::info_span!("Build Mutation Document", internal.visibility = "user")
                .in_scope(|| build_mutation_document(&request, configuration))?;

        let query = serde_json::to_string_pretty(&GraphQLRequest::new(
            &operation.query,
            &operation.variables,
        ))
        .map_err(ExplainError::new)?;

        let details = BTreeMap::from_iter(vec![
            ("SQL Query".to_string(), operation.query),
            ("Execution Plan".to_string(), query),
            (
                "Headers".to_string(),
                serde_json::to_string(&operation.headers).expect("should convert headers to json"),
            ),
        ]);

        Ok(JsonResponse::Value(models::ExplainResponse { details }))
    }

    async fn mutation(
        configuration: &Self::Configuration,
        state: &Self::State,
        request: models::MutationRequest,
    ) -> Result<JsonResponse<models::MutationResponse>, MutationError> {
        let operation =
            tracing::info_span!("Build Mutation Document", internal.visibility = "user")
                .in_scope(|| build_mutation_document(&request, configuration))?;

        let client = state
            .client(configuration)
            .await
            .map_err(MutationError::new)?;

        let execution_span =
            tracing::info_span!("Execute GraphQL Mutation", internal.visibility = "user");

        let (headers, response) = execute_graphql::<IndexMap<String, serde_json::Value>>(
            &operation.query,
            operation.variables,
            &configuration.connection.endpoint,
            &operation.headers,
            &client,
            &configuration
                .response
                .forward_headers
                .clone()
                .unwrap_or_default(),
        )
        .instrument(execution_span)
        .await
        .map_err(MutationError::new)?;

        tracing::info_span!("Process Response").in_scope(|| {
            if let Some(errors) = response.errors {
                Err(MutationError::new_unprocessable_content(&errors[0].message)
                    .with_details(serde_json::json!({ "errors": errors })))
            } else if let Some(mut data) = response.data {
                let forward_response_headers = configuration.response.forward_headers.is_some();

                let operation_results = request
                    .operations
                    .iter()
                    .enumerate()
                    .map(|(index, operation)| match operation {
                        models::MutationOperation::Procedure { .. } => Ok({
                            let alias = format!("procedure_{index}");
                            let result = data
                                .get_mut(&alias)
                                .map(|val| mem::replace(val, serde_json::Value::Null))
                                .unwrap_or(serde_json::Value::Null);
                            let result = if forward_response_headers {
                                serde_json::to_value(BTreeMap::from_iter(vec![
                                    (
                                        configuration.response.headers_field.to_string(),
                                        serde_json::to_value(&headers)?,
                                    ),
                                    (configuration.response.response_field.to_string(), result),
                                ]))?
                            } else {
                                result
                            };

                            MutationOperationResults::Procedure { result }
                        }),
                    })
                    .collect::<Result<Vec<_>, serde_json::Error>>()
                    .map_err(MutationError::new)?;

                Ok(JsonResponse::Value(models::MutationResponse {
                    operation_results,
                }))
            } else {
                Err(MutationError::new_unprocessable_content(
                    &"No data or errors in response",
                ))
            }
        })
    }

    async fn query(
        configuration: &Self::Configuration,
        state: &Self::State,
        request: models::QueryRequest,
    ) -> Result<JsonResponse<models::QueryResponse>, QueryError> {
        let operation = tracing::info_span!("Build Query Document", internal.visibility = "user")
            .in_scope(|| build_query_document(&request, configuration))?;

        let client = state.client(configuration).await.map_err(QueryError::new)?;

        let execution_span =
            tracing::info_span!("Execute GraphQL Query", internal.visibility = "user");

        let (headers, response) = execute_graphql::<IndexMap<String, RowFieldValue>>(
            &operation.query,
            operation.variables,
            &configuration.connection.endpoint,
            &operation.headers,
            &client,
            &configuration
                .response
                .forward_headers
                .clone()
                .unwrap_or_default(),
        )
        .instrument(execution_span)
        .await
        .map_err(QueryError::new)?;

        tracing::info_span!("Process Response").in_scope(|| {
            if let Some(errors) = response.errors {
                Err(QueryError::new_unprocessable_content(&errors[0].message)
                    .with_details(serde_json::json!({ "errors": errors })))
            } else if let Some(data) = response.data {
                let forward_response_headers = configuration.response.forward_headers.is_some();

                let row = if forward_response_headers {
                    let headers = serde_json::to_value(headers).map_err(QueryError::new)?;
                    let data = serde_json::to_value(data).map_err(QueryError::new)?;

                    IndexMap::from_iter(vec![
                        (
                            configuration.response.headers_field.to_string(),
                            RowFieldValue(headers),
                        ),
                        (
                            configuration.response.response_field.to_string(),
                            RowFieldValue(data),
                        ),
                    ])
                } else {
                    data
                };

                Ok(JsonResponse::Value(models::QueryResponse(vec![RowSet {
                    aggregates: None,
                    rows: Some(vec![row]),
                }])))
            } else {
                Err(QueryError::new_unprocessable_content(
                    &"No data or errors in response",
                ))
            }
        })
    }
}
