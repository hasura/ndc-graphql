use config_file::{RequestConfigFile, ResponseConfigFile};
use schema::SchemaDefinition;
use std::collections::BTreeMap;
pub mod config_file;
pub mod schema;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub connection: ConnectionConfig,
    pub request: RequestConfig,
    pub response: ResponseConfig,
    pub schema: SchemaDefinition,
}

#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    pub endpoint: String,
    pub headers: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct RequestConfig {
    pub headers_argument: String,
    pub headers_type_name: String,
    pub forward_headers: Vec<String>,
}
#[derive(Debug, Clone)]
pub struct ResponseConfig {
    pub headers_field: String,
    pub response_field: String,
    pub type_name_prefix: String,
    pub type_name_suffix: String,
    pub forward_headers: Vec<String>,
}

impl Default for RequestConfig {
    fn default() -> Self {
        Self {
            headers_argument: "_headers".to_owned(),
            headers_type_name: "_HeaderMap".to_owned(),
            forward_headers: vec![],
        }
    }
}

impl Default for ResponseConfig {
    fn default() -> Self {
        Self {
            headers_field: "headers".to_owned(),
            response_field: "response".to_owned(),
            type_name_prefix: "_".to_owned(),
            type_name_suffix: "Response".to_owned(),
            forward_headers: vec![],
        }
    }
}

impl From<RequestConfigFile> for RequestConfig {
    fn from(value: RequestConfigFile) -> Self {
        RequestConfig {
            headers_argument: value
                .headers_argument
                .unwrap_or_else(|| Self::default().headers_argument),
            headers_type_name: value
                .headers_type_name
                .unwrap_or_else(|| Self::default().headers_type_name),
            forward_headers: value.forward_headers.unwrap_or_default(),
        }
    }
}
impl From<ResponseConfigFile> for ResponseConfig {
    fn from(value: ResponseConfigFile) -> Self {
        ResponseConfig {
            headers_field: value
                .headers_field
                .unwrap_or_else(|| Self::default().headers_field),
            response_field: value
                .response_field
                .unwrap_or_else(|| Self::default().response_field),
            type_name_prefix: value
                .type_name_prefix
                .unwrap_or_else(|| Self::default().type_name_prefix),
            type_name_suffix: value
                .type_name_suffix
                .unwrap_or_else(|| Self::default().type_name_suffix),
            forward_headers: value.forward_headers.unwrap_or_default(),
        }
    }
}

impl ResponseConfig {
    pub fn query_response_type_name(&self, query: &str) -> String {
        format!(
            "{}{}Query{}",
            self.type_name_prefix, query, self.type_name_suffix
        )
    }
    pub fn mutation_response_type_name(&self, mutation: &str) -> String {
        format!(
            "{}{}Mutation{}",
            self.type_name_prefix, mutation, self.type_name_suffix
        )
    }
}
