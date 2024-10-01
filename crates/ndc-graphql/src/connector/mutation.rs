use super::state::ServerState;
use crate::query_builder::build_mutation_document;
use common::{
    client::{execute_graphql, GraphQLRequest},
    config::ServerConfig,
};
use http::StatusCode;
use indexmap::IndexMap;
use ndc_sdk::{connector::ErrorResponse, models};
use std::{collections::BTreeMap, mem};
use tracing::{Instrument, Level};

pub async fn handle_mutation_explain(
    configuration: &ServerConfig,
    _state: &ServerState,
    request: models::MutationRequest,
) -> Result<models::ExplainResponse, ErrorResponse> {
    let operation = tracing::info_span!("Build Mutation Document", internal.visibility = "user")
        .in_scope(|| build_mutation_document(&request, configuration))?;

    let query =
        serde_json::to_string_pretty(&GraphQLRequest::new(&operation.query, &operation.variables))
            .map_err(ErrorResponse::from_error)?;

    let details = BTreeMap::from_iter(vec![
        ("SQL Query".to_string(), operation.query),
        ("Execution Plan".to_string(), query),
        (
            "Headers".to_string(),
            serde_json::to_string(&operation.headers).expect("should convert headers to json"),
        ),
    ]);

    Ok(models::ExplainResponse { details })
}

pub async fn handle_mutation(
    configuration: &ServerConfig,
    state: &ServerState,
    request: models::MutationRequest,
) -> Result<models::MutationResponse, ErrorResponse> {
    #[cfg(debug_assertions)]
    {
        // this block only present in debug builds, to avoid leaking sensitive information
        let request_string = serde_json::to_string(&request).map_err(ErrorResponse::from_error)?;
        tracing::event!(Level::DEBUG, "Incoming IR" = request_string);
    }

    let operation = tracing::info_span!("Build Mutation Document", internal.visibility = "user")
        .in_scope(|| build_mutation_document(&request, configuration))?;

    let client = state
        .client(configuration)
        .await
        .map_err(ErrorResponse::from_error)?;

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
    .map_err(ErrorResponse::from_error)?;

    tracing::info_span!("Process Response").in_scope(|| {
        if let Some(errors) = response.errors {
            Err(ErrorResponse::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Errors in graphql query response".to_string(),
                serde_json::json!({
                    "errors": errors
                }),
            ))
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
                .map_err(ErrorResponse::from_error)?;

            Ok(models::MutationResponse { operation_results })
        } else {
            Err(ErrorResponse::new_internal_with_details(
                serde_json::json!({
                    "message": "No data or errors in response"
                }),
            ))
        }
    })
}
