use ndc_models::{ArgumentName, FieldName, ScalarTypeName};
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
    /// Connection Configuration for introspection.
    pub introspection: ConnectionConfigFile,
    /// Connection configuration for query execution.
    pub execution: ConnectionConfigFile,
    /// Optional configuration for requests.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub request: Option<RequestConfigFile>,
    /// Optional configuration for responses.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub response: Option<ResponseConfigFile>,
}

impl Default for ServerConfigFile {
    fn default() -> Self {
        Self {
            json_schema: CONFIG_SCHEMA_FILE_NAME.to_owned(),
            execution: ConnectionConfigFile::default(),
            introspection: ConnectionConfigFile::default(),
            request: None,
            response: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConnectionConfigFile {
    /// Target GraphQL endpoint URL
    pub endpoint: ConfigValue,
    /// Static headers to include with each request
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub headers: BTreeMap<String, ConfigValue>,
}

impl Default for ConnectionConfigFile {
    fn default() -> Self {
        Self {
            endpoint: ConfigValue::ValueFromEnv("GRAPHQL_ENDPOINT".to_string()),
            headers: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequestConfigFile {
    /// Name of the headers argument.
    /// Must not conflict with any arguments of root fields in the target schema.
    /// Defaults to "_headers", set to a different value if there is a conflict.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub headers_argument: Option<ArgumentName>,
    /// Name of the headers argument type.
    /// Must not conflict with other types in the target schema.
    /// Defaults to "_HeaderMap", set to a different value if there is a conflict.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub headers_type_name: Option<ScalarTypeName>,
    /// List of headers to forward from the request.
    /// Defaults to [], AKA no headers/disabled.
    /// Supports glob patterns eg. "X-Hasura-*".
    /// Enabling this requires additional configuration on the ddn side, see docs for more.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub forward_headers: Option<Vec<String>>,
}
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ResponseConfigFile {
    /// Name of the headers field in the response type.
    /// Defaults to "headers".
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub headers_field: Option<FieldName>,
    /// Name of the response field in the response type.
    /// Defaults to "response".
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub response_field: Option<FieldName>,
    /// Prefix for response type names.
    /// Defaults to "_".
    /// Generated response type names must be unique once prefix and suffix are applied.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub type_name_prefix: Option<String>,
    /// Suffix for response type names.
    /// Defaults to "Response".
    /// Generated response type names must be unique once prefix and suffix are applied.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub type_name_suffix: Option<String>,
    /// List of headers to forward from the response.
    /// Defaults to [], AKA no headers/disabled.
    /// Supports glob patterns eg. "X-Hasura-*".
    /// Enabling this requires additional configuration on the ddn side, see docs for more.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub forward_headers: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum ConfigValue {
    /// A static string value
    #[serde(rename = "value")]
    Value(String),
    /// A reference to an environment variable, from which the value will be read at runtime
    #[serde(rename = "valueFromEnv")]
    ValueFromEnv(String),
}

impl Default for RequestConfigFile {
    fn default() -> Self {
        Self {
            headers_argument: None,
            headers_type_name: None,
            forward_headers: Some(vec![]),
        }
    }
}

impl Default for ResponseConfigFile {
    fn default() -> Self {
        Self {
            headers_field: None,
            response_field: None,
            type_name_prefix: None,
            type_name_suffix: None,
            forward_headers: Some(vec![]),
        }
    }
}
