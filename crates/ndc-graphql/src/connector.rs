use self::state::ServerState;
use crate::query_builder::{build_mutation_document, build_query_document};
use async_trait::async_trait;
use common::{
    capabilities::capabilities,
    client::{execute_graphql, GraphQLRequest},
    config::ServerConfig,
    schema_response::schema_response,
};
use indexmap::IndexMap;
use ndc_sdk::{
    connector::{self, Connector, MutationError, QueryError},
    json_response::JsonResponse,
    models::{self, FieldName},
};
use std::{collections::BTreeMap, mem};
use tracing::{Instrument, Level};
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
    ) -> connector::Result<()> {
        Ok(())
    }

    async fn get_capabilities() -> models::Capabilities {
        capabilities()
    }

    async fn get_schema(
        configuration: &Self::Configuration,
    ) -> connector::Result<JsonResponse<models::SchemaResponse>> {
        Ok(JsonResponse::Value(schema_response(
            &configuration.schema,
            &configuration.request,
            &configuration.response,
        )))
    }

    async fn query_explain(
        configuration: &Self::Configuration,
        _state: &Self::State,
        request: models::QueryRequest,
    ) -> connector::Result<JsonResponse<models::ExplainResponse>> {
        let operation = tracing::info_span!("Build Query Document", internal.visibility = "user")
            .in_scope(|| build_query_document(&request, configuration))
            .map_err(|err| QueryError::new_invalid_request(&err))?;

        let query = serde_json::to_string_pretty(&GraphQLRequest::new(
            &operation.query,
            &operation.variables,
        ))
        .map_err(|err| QueryError::new_invalid_request(&err))?;

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
    ) -> connector::Result<JsonResponse<models::ExplainResponse>> {
        let operation =
            tracing::info_span!("Build Mutation Document", internal.visibility = "user")
                .in_scope(|| build_mutation_document(&request, configuration))
                .map_err(|err| MutationError::new_invalid_request(&err))?;

        let query = serde_json::to_string_pretty(&GraphQLRequest::new(
            &operation.query,
            &operation.variables,
        ))
        .map_err(|err| MutationError::new_invalid_request(&err))?;

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
    ) -> connector::Result<JsonResponse<models::MutationResponse>> {
        #[cfg(debug_assertions)]
        {
            // this block only present in debug builds, to avoid leaking sensitive information
            let request_string = serde_json::to_string(&request)
                .map_err(|err| MutationError::new_invalid_request(&err))?;
            tracing::event!(Level::DEBUG, "Incoming IR" = request_string);
        }

        let operation =
            tracing::info_span!("Build Mutation Document", internal.visibility = "user").in_scope(
                || {
                    build_mutation_document(&request, configuration)
                        .map_err(|err| MutationError::new_invalid_request(&err))
                },
            )?;

        let client = state
            .client(configuration)
            .await
            .map_err(|err| MutationError::new_invalid_request(&err))?;

        let execution_span =
            tracing::info_span!("Execute GraphQL Mutation", internal.visibility = "user");

        let (headers, response) = execute_graphql::<IndexMap<String, serde_json::Value>>(
            &operation.query,
            operation.variables,
            &configuration.connection.endpoint,
            &operation.headers,
            &client,
            &configuration.response.forward_headers,
        )
        .instrument(execution_span)
        .await
        .map_err(|err| MutationError::new_invalid_request(&err))?;

        Ok(tracing::info_span!("Process Response").in_scope(|| {
            if let Some(errors) = response.errors {
                Err(MutationError::new_unprocessable_content(&errors[0].message)
                    .with_details(serde_json::json!({ "errors": errors })))
            } else if let Some(mut data) = response.data {
                let forward_response_headers = !configuration.response.forward_headers.is_empty();

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

                            models::MutationOperationResults::Procedure { result }
                        }),
                    })
                    .collect::<Result<Vec<_>, serde_json::Error>>()
                    .map_err(|err| MutationError::new_invalid_request(&err))?;

                Ok(JsonResponse::Value(models::MutationResponse {
                    operation_results,
                }))
            } else {
                Err(MutationError::new_unprocessable_content(
                    &"No data or errors in response",
                ))
            }
        })?)
    }

    async fn query(
        configuration: &Self::Configuration,
        state: &Self::State,
        request: models::QueryRequest,
    ) -> connector::Result<JsonResponse<models::QueryResponse>> {
        #[cfg(debug_assertions)]
        {
            // this block only present in debug builds, to avoid leaking sensitive information
            let request_string = serde_json::to_string(&request)
                .map_err(|err| QueryError::new_invalid_request(&err))?;
            tracing::event!(Level::DEBUG, "Incoming IR" = request_string);
        }

        let operation = tracing::info_span!("Build Query Document", internal.visibility = "user")
            .in_scope(|| {
            build_query_document(&request, configuration)
                .map_err(|err| QueryError::new_invalid_request(&err))
        })?;

        let client = state
            .client(configuration)
            .await
            .map_err(|err| QueryError::new_invalid_request(&err))?;

        let execution_span =
            tracing::info_span!("Execute GraphQL Query", internal.visibility = "user");

        let (headers, response) = execute_graphql::<IndexMap<FieldName, models::RowFieldValue>>(
            &operation.query,
            operation.variables,
            &configuration.connection.endpoint,
            &operation.headers,
            &client,
            &configuration.response.forward_headers,
        )
        .instrument(execution_span)
        .await
        .map_err(|err| QueryError::new_invalid_request(&err))?;

        Ok(tracing::info_span!("Process Response").in_scope(|| {
            if let Some(errors) = response.errors {
                Err(QueryError::new_unprocessable_content(&errors[0].message)
                    .with_details(serde_json::json!({ "errors": errors })))
            } else if let Some(data) = response.data {
                let forward_response_headers = !configuration.response.forward_headers.is_empty();

                let row = if forward_response_headers {
                    let headers = serde_json::to_value(headers)
                        .map_err(|err| QueryError::new_invalid_request(&err))?;
                    let data = serde_json::to_value(data)
                        .map_err(|err| QueryError::new_invalid_request(&err))?;

                    IndexMap::from_iter(vec![
                        (
                            configuration.response.headers_field.to_string().into(),
                            models::RowFieldValue(headers),
                        ),
                        (
                            configuration.response.response_field.to_string().into(),
                            models::RowFieldValue(data),
                        ),
                    ])
                } else {
                    data
                };

                Ok(JsonResponse::Value(models::QueryResponse(vec![
                    models::RowSet {
                        aggregates: None,
                        rows: Some(vec![row]),
                    },
                ])))
            } else {
                Err(QueryError::new_unprocessable_content(
                    &"No data or errors in response",
                ))
            }
        })?)
    }
}
