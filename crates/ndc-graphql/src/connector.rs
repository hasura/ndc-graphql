use std::{collections::BTreeMap, mem, path::Path};

use async_trait::async_trait;
use common::{
    client::{execute_graphql, GraphQLRequest},
    config::ServerConfig,
};
use graphql_parser::{parse_schema, schema::Document};
use indexmap::IndexMap;
use ndc_sdk::{
    connector::{
        Connector, ConnectorSetup, ExplainError, FetchMetricsError, HealthError,
        InitializationError, MutationError, ParseError, QueryError, SchemaError,
    },
    json_response::JsonResponse,
    models::{
        self, CapabilitiesResponse, LeafCapability, MutationOperationResults, RowFieldValue, RowSet,
    },
};
use schema::schema_from_graphql_document;
use tracing::Instrument;

use crate::query_builder::{build_mutation_document, build_query_document};

use self::{configuration::read_configuration, state::ServerState};

mod configuration;
mod schema;
mod state;

#[derive(Debug, Default, Clone)]
pub struct GraphQLConnector;

#[async_trait]
impl ConnectorSetup for GraphQLConnector {
    type Connector = Self;

    async fn parse_configuration(
        &self,
        configuration_dir: impl AsRef<Path> + Send,
    ) -> Result<<Self as Connector>::Configuration, ParseError> {
        read_configuration(&configuration_dir.as_ref().to_path_buf()).await
    }

    async fn try_init_state(
        &self,
        configuration: &<Self as Connector>::Configuration,
        _metrics: &mut prometheus::Registry,
    ) -> Result<<Self as Connector>::State, InitializationError> {
        Ok(ServerState::new(&configuration))
    }
}

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
        let schema_document = parse_schema(&configuration.schema_string)
            .map_err(|err| SchemaError::Other(err.to_string().into()))?;
        Ok(JsonResponse::Value(schema_from_graphql_document(
            &schema_document,
        )))
    }

    async fn query_explain(
        configuration: &Self::Configuration,
        _state: &Self::State,
        request: models::QueryRequest,
    ) -> Result<JsonResponse<models::ExplainResponse>, ExplainError> {
        let schema_document = tracing::info_span!(
            "Parse GraphQL Schema Document",
            internal.visibility = "user"
        )
        .in_scope(|| -> Result<Document<String>, _> { parse_schema(&configuration.schema_string) })
        .map_err(|err| ExplainError::Other(err.to_string().into()))?;

        let (document, variables) =
            tracing::info_span!("Build Query Document", internal.visibility = "user")
                .in_scope(|| build_query_document(&request, &schema_document))?;

        let query = serde_json::to_string_pretty(&GraphQLRequest::new(&document, &variables))
            .map_err(|err| ExplainError::InvalidRequest(err.to_string()))?;

        let details = BTreeMap::from_iter(vec![
            ("SQL Query".to_string(), document.to_owned()),
            ("Execution Plan".to_string(), query),
        ]);

        Ok(JsonResponse::Value(models::ExplainResponse { details }))
    }

    async fn mutation_explain(
        configuration: &Self::Configuration,
        _state: &Self::State,
        request: models::MutationRequest,
    ) -> Result<JsonResponse<models::ExplainResponse>, ExplainError> {
        let schema_document = tracing::info_span!(
            "Parse GraphQL Schema Document",
            internal.visibility = "user"
        )
        .in_scope(|| -> Result<Document<String>, _> { parse_schema(&configuration.schema_string) })
        .map_err(|err| ExplainError::Other(err.to_string().into()))?;
        let (document, variables) =
            tracing::info_span!("Build Mutation Document", internal.visibility = "user")
                .in_scope(|| build_mutation_document(&request, &schema_document))?;

        let query = serde_json::to_string_pretty(&GraphQLRequest::new(&document, &variables))
            .map_err(|err| ExplainError::InvalidRequest(err.to_string()))?;

        let details = BTreeMap::from_iter(vec![
            ("SQL Query".to_string(), document.to_owned()),
            ("Execution Plan".to_string(), query),
        ]);

        Ok(JsonResponse::Value(models::ExplainResponse { details }))
    }

    async fn mutation(
        configuration: &Self::Configuration,
        state: &Self::State,
        request: models::MutationRequest,
    ) -> Result<JsonResponse<models::MutationResponse>, MutationError> {
        let schema_document = tracing::info_span!(
            "Parse GraphQL Schema Document",
            internal.visibility = "user"
        )
        .in_scope(|| -> Result<Document<String>, _> { parse_schema(&configuration.schema_string) })
        .map_err(|err| MutationError::Other(err.to_string().into()))?;

        let (document, variables) =
            tracing::info_span!("Build Mutation Document", internal.visibility = "user")
                .in_scope(|| build_mutation_document(&request, &schema_document))?;

        let client = state
            .client(&configuration)
            .await
            .map_err(|err| MutationError::Other(err.to_string().into()))?;

        let execution_span =
            tracing::info_span!("Execute GraphQL Mutation", internal.visibility = "user");

        let response = execute_graphql::<serde_json::Value>(
            &document.to_string(),
            variables,
            &client,
            &configuration.connection,
        )
        .instrument(execution_span)
        .await
        .map_err(|err| MutationError::Other(err.to_string().into()))?;

        tracing::info_span!("Process Response").in_scope(|| {
            if let Some(errors) = response.errors {
                Err(MutationError::InvalidRequest(
                    serde_json::to_string(&errors)
                        .map_err(|err| MutationError::Other(err.into()))?
                        .into(),
                ))
            } else if let Some(mut data) = response.data {
                let operation_results = request
                    .operations
                    .iter()
                    .enumerate()
                    .map(|(index, operation)| match operation {
                        models::MutationOperation::Procedure { .. } => {
                            let alias = format!("procedure_{index}");
                            // data object keys will only get consumed once, avoid unecessary cloning
                            let result = data
                                .get_mut(alias)
                                .map(|val| mem::replace(val, serde_json::Value::Null))
                                .unwrap_or(serde_json::Value::Null);

                            MutationOperationResults::Procedure { result }
                        }
                    })
                    .collect();

                Ok(JsonResponse::Value(models::MutationResponse {
                    operation_results,
                }))
            } else {
                Err(MutationError::UnprocessableContent(
                    "No data or errors in response".into(),
                ))
            }
        })
    }

    async fn query(
        configuration: &Self::Configuration,
        state: &Self::State,
        request: models::QueryRequest,
    ) -> Result<JsonResponse<models::QueryResponse>, QueryError> {
        let schema_document = tracing::info_span!(
            "Parse GraphQL Schema Document",
            internal.visibility = "user"
        )
        .in_scope(|| -> Result<Document<String>, _> { parse_schema(&configuration.schema_string) })
        .map_err(|err| QueryError::Other(err.to_string().into()))?;

        let (document, variables) =
            tracing::info_span!("Build Query Document", internal.visibility = "user")
                .in_scope(|| build_query_document(&request, &schema_document))?;

        let client = state
            .client(&configuration)
            .await
            .map_err(|err| QueryError::Other(err.to_string().into()))?;

        let execution_span =
            tracing::info_span!("Execute GraphQL Query", internal.visibility = "user");

        let response = execute_graphql::<IndexMap<String, RowFieldValue>>(
            &document.to_string(),
            variables,
            &client,
            &configuration.connection,
        )
        .instrument(execution_span)
        .await
        .map_err(|err| QueryError::Other(err.to_string().into()))?;

        tracing::info_span!("Process Response").in_scope(|| {
            if let Some(errors) = response.errors {
                Err(QueryError::Other(
                    serde_json::to_string(&errors)
                        .map_err(|err| QueryError::Other(err.into()))?
                        .into(),
                ))
            } else if let Some(data) = response.data {
                Ok(JsonResponse::Value(models::QueryResponse(vec![RowSet {
                    aggregates: None,
                    rows: Some(vec![data]),
                }])))
            } else {
                Err(QueryError::UnprocessableContent(
                    "No data or errors in response".into(),
                ))
            }
        })
    }
}
