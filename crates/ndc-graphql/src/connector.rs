use self::state::ServerState;
use async_trait::async_trait;
use common::{capabilities::capabilities, config::ServerConfig, schema_response::schema_response};
use mutation::{handle_mutation, handle_mutation_explain};
use ndc_sdk::{
    connector::{self, Connector},
    json_response::JsonResponse,
    models,
};
use query::{handle_query, handle_query_explain};
mod mutation;
mod query;
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

    fn connector_name() -> &'static str {
        "ndc_graphql"
    }

    fn connector_version() -> &'static str {
        env!("CARGO_PKG_VERSION")
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
        state: &Self::State,
        request: models::QueryRequest,
    ) -> connector::Result<JsonResponse<models::ExplainResponse>> {
        Ok(JsonResponse::Value(
            handle_query_explain(configuration, state, request).await?,
        ))
    }

    async fn mutation_explain(
        configuration: &Self::Configuration,
        state: &Self::State,
        request: models::MutationRequest,
    ) -> connector::Result<JsonResponse<models::ExplainResponse>> {
        Ok(JsonResponse::Value(
            handle_mutation_explain(configuration, state, request).await?,
        ))
    }

    async fn mutation(
        configuration: &Self::Configuration,
        state: &Self::State,
        request: models::MutationRequest,
    ) -> connector::Result<JsonResponse<models::MutationResponse>> {
        Ok(JsonResponse::Value(
            handle_mutation(configuration, state, request).await?,
        ))
    }

    async fn query(
        configuration: &Self::Configuration,
        state: &Self::State,
        request: models::QueryRequest,
    ) -> connector::Result<JsonResponse<models::QueryResponse>> {
        Ok(JsonResponse::Value(
            handle_query(configuration, state, request).await?,
        ))
    }
}
