use crate::config::ConnectionConfig;
use glob_match::glob_match;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::Serialize;
use std::{collections::BTreeMap, fmt::Debug};

pub fn get_http_client(
    _connection_config: &ConnectionConfig,
) -> Result<reqwest::Client, reqwest::Error> {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

    let client = reqwest::Client::builder()
        .default_headers(headers)
        .build()?;

    Ok(client)
}

pub async fn execute_graphql<T: serde::de::DeserializeOwned>(
    query: &str,
    variables: BTreeMap<String, serde_json::Value>,
    endpoint: &str,
    headers: &BTreeMap<String, String>,
    client: &reqwest::Client,
    return_headers: &Vec<String>,
) -> Result<(BTreeMap<String, String>, graphql_client::Response<T>), reqwest::Error> {
    let mut request = client.post(endpoint);

    for (header_name, header_value) in headers {
        request = request.header(header_name, header_value);
    }

    let request_body = GraphQLRequest::new(query, &variables);

    let request = request.json(&request_body);

    let response = request.send().await?;
    let headers = response
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            for pattern in return_headers {
                if glob_match(&pattern.to_lowercase(), &name.as_str().to_lowercase()) {
                    return Some((
                        name.to_string(),
                        value.to_str().unwrap_or_default().to_string(),
                    ));
                }
            }
            None
        })
        .collect();

    let response: graphql_client::Response<T> = response.error_for_status()?.json().await?;

    Ok((headers, response))
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
