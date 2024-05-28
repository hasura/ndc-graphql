use crate::config::ConnectionConfig;
use serde::Serialize;
use std::{collections::BTreeMap, error::Error, fmt::Debug};

pub fn get_http_client(
    _connection_config: &ConnectionConfig,
) -> Result<reqwest::Client, Box<dyn std::error::Error>> {
    // todo: we could make client come preconfigured with some headers such as for username and password?
    let client = reqwest::Client::builder().build()?;
    Ok(client)
}

pub async fn execute_graphql<T: serde::de::DeserializeOwned>(
    query: &str,
    variables: BTreeMap<String, serde_json::Value>,
    client: &reqwest::Client,
    connection_config: &ConnectionConfig,
) -> Result<graphql_client::Response<T>, Box<dyn Error>> {
    let mut request = client.post(&connection_config.endpoint);

    for (header_name, header_value) in &connection_config.headers {
        request = request.header(header_name, &header_value.value);
    }

    let request_body = GraphQLRequest::new(query, &variables);

    let request = request.json(&request_body);

    let response = request.send().await?;

    if response.error_for_status_ref().is_err() {
        return Err(response.text().await?.into());
    }

    let response: graphql_client::Response<T> = response.json().await?;

    Ok(response)
}

#[derive(Debug, Serialize)]
pub struct GraphQLRequest<'a> {
    query: &'a str,
    variables: &'a BTreeMap<String, serde_json::Value>,
}

impl<'a> GraphQLRequest<'a> {
    pub fn new(query: &'a str, variables: &'a BTreeMap<String, serde_json::Value>) -> Self {
        Self { query, variables }
    }
}
