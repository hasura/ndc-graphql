use super::state::ServerState;
use crate::query_builder::build_query_document;
use common::{
    client::{execute_graphql, GraphQLRequest},
    config::ServerConfig,
};
use http::StatusCode;
use indexmap::IndexMap;
use ndc_sdk::{
    connector::{ErrorResponse, QueryError},
    models::{self, FieldName},
};
use std::collections::BTreeMap;
use tracing::{Instrument, Level};

pub async fn handle_query_explain(
    configuration: &ServerConfig,
    _state: &ServerState,
    request: models::QueryRequest,
) -> Result<models::ExplainResponse, ErrorResponse> {
    let operation = tracing::info_span!("Build Query Document", internal.visibility = "user")
        .in_scope(|| build_query_document(&request, configuration))?;

    let query =
        serde_json::to_string_pretty(&GraphQLRequest::new(&operation.query, &operation.variables))
            .map_err(|err| QueryError::new_invalid_request(&err))?;

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

pub async fn handle_query(
    configuration: &ServerConfig,
    state: &ServerState,
    request: models::QueryRequest,
) -> Result<models::QueryResponse, ErrorResponse> {
    #[cfg(debug_assertions)]
    {
        // this block only present in debug builds, to avoid leaking sensitive information
        let request_string = serde_json::to_string(&request).map_err(ErrorResponse::from_error)?;
        tracing::event!(Level::DEBUG, "Incoming IR" = request_string);
    }

    let operation = tracing::info_span!("Build Query Document", internal.visibility = "user")
        .in_scope(|| build_query_document(&request, configuration))?;

    let client = state
        .client(configuration)
        .await
        .map_err(ErrorResponse::from_error)?;

    let execution_span = tracing::info_span!("Execute GraphQL Query", internal.visibility = "user");

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
        } else if let Some(data) = response.data {
            let forward_response_headers = !configuration.response.forward_headers.is_empty();

            let row = if forward_response_headers {
                let headers = serde_json::to_value(headers)
                    .map_err(|err| QueryError::new_unprocessable_content(&err))?;
                let data = serde_json::to_value(data)
                    .map_err(|err| QueryError::new_unprocessable_content(&err))?;

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

            Ok(models::QueryResponse(vec![models::RowSet {
                aggregates: None,
                rows: Some(vec![row]),
            }]))
        } else {
            Err(ErrorResponse::new_internal_with_details(
                serde_json::json!({
                    "message": "No data or errors in response"
                }),
            ))
        }
    })
}
