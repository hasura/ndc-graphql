use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const SCHEMA_FILE_NAME: &str = "schema.graphql";
pub const CONFIG_FILE_NAME: &str = "configuration.json";
pub const CONFIG_SCHEMA_FILE_NAME: &str = "configuration.schema.json";

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ServerConfigFile {
    #[serde(rename = "$schema")]
    pub json_schema: String,
    /// Connection configuration for query execution
    pub execution: ConnectionConfigFile,
    /// Optional Connection Configuration for introspection
    pub introspection: ConnectionConfigFile,
    /// Optional configuration for requests
    pub request: RequestConfig<Option<String>>,
    /// Optional configuration for responses
    pub response: ResponseConfig<Option<String>>,
}

impl Default for ServerConfigFile {
    fn default() -> Self {
        Self {
            json_schema: CONFIG_SCHEMA_FILE_NAME.to_owned(),
            execution: ConnectionConfigFile::default(),
            introspection: ConnectionConfigFile::default(),
            request: RequestConfig::default(),
            response: ResponseConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConnectionConfigFile {
    pub endpoint: ConfigValue,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub headers: BTreeMap<String, ConfigValue>,
}

impl Default for ConnectionConfigFile {
    fn default() -> Self {
        Self {
            endpoint: ConfigValue::Value("".to_string()),
            headers: BTreeMap::from_iter(vec![
                (
                    "Content-Type".to_owned(),
                    ConfigValue::Value("application/json".to_string()),
                ),
                (
                    "Authorization".to_owned(),
                    ConfigValue::ValueFromEnv("GRAPHQL_ENDPOINT_AUTHORIZATION".to_string()),
                ),
            ]),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequestConfig<T> {
    /// Name of the headers argument
    /// Must not conflict with any arguments of root fields in the target schema
    /// Defaults to "_headers", set to a different value if there is a conflict
    pub headers_argument: T,
    /// Name of the headers argument type
    /// Must not conflict with other types in the target schema
    /// Defaults to "_HeaderMap", set to a different value if there is a conflict
    pub headers_type_name: T,
    /// List of headers to from the request
    /// Defaults to ["*"], AKA all headers
    /// Supports glob patterns eg. "X-Hasura-*"
    pub forward_headers: Option<Vec<String>>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResponseConfig<T> {
    /// Name of the headers field in the response type
    /// Defaults to "headers"
    pub headers_field: T,
    /// Name of the response field in the response type
    /// Defaults to "response"
    pub response_field: T,
    /// Prefix for response type names
    /// Defaults to "_"
    /// Generated response type names must be unique once prefix and suffix are applied
    pub type_name_prefix: T,
    /// Suffix for response type names
    /// Defaults to "Response"
    /// Generated response type names must be unique once prefix and suffix are applied
    pub type_name_suffix: T,
    /// List of headers to from the response
    /// Defaults to ["*"], AKA all headers
    /// Supports glob patterns eg. "X-Hasura-*"
    pub forward_headers: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum ConfigValue {
    #[serde(rename = "value")]
    Value(String),
    #[serde(rename = "valueFromEnv")]
    ValueFromEnv(String),
}

impl Default for RequestConfig<String> {
    fn default() -> Self {
        Self {
            headers_argument: "_headers".to_owned(),
            headers_type_name: "_HeaderMap".to_owned(),
            forward_headers: Some(vec!["Authorization".to_owned(), "X-Hasura-*".to_owned()]),
        }
    }
}

impl Default for RequestConfig<Option<String>> {
    fn default() -> Self {
        Self {
            headers_argument: None,
            headers_type_name: None,
            forward_headers: Some(vec!["*".to_owned()]),
        }
    }
}

impl Default for ResponseConfig<String> {
    fn default() -> Self {
        Self {
            headers_field: "headers".to_owned(),
            response_field: "response".to_owned(),
            type_name_prefix: "_".to_owned(),
            type_name_suffix: "Response".to_owned(),
            forward_headers: Some(vec!["Set-Cookie".to_owned()]),
        }
    }
}

impl Default for ResponseConfig<Option<String>> {
    fn default() -> Self {
        Self {
            headers_field: None,
            response_field: None,
            type_name_prefix: None,
            type_name_suffix: None,
            forward_headers: Some(vec!["Set-Cookie".to_owned()]),
        }
    }
}

impl From<RequestConfig<Option<String>>> for RequestConfig<String> {
    fn from(value: RequestConfig<Option<String>>) -> Self {
        RequestConfig {
            headers_argument: value
                .headers_argument
                .unwrap_or_else(|| Self::default().headers_argument),
            headers_type_name: value
                .headers_type_name
                .unwrap_or_else(|| Self::default().headers_type_name),
            forward_headers: value.forward_headers.and_then(|forward_headers| {
                if forward_headers.is_empty() {
                    None
                } else {
                    Some(forward_headers)
                }
            }),
        }
    }
}
impl From<ResponseConfig<Option<String>>> for ResponseConfig<String> {
    fn from(value: ResponseConfig<Option<String>>) -> Self {
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
            forward_headers: value.forward_headers.and_then(|forward_headers| {
                if forward_headers.is_empty() {
                    None
                } else {
                    Some(forward_headers)
                }
            }),
        }
    }
}
