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
    pub connection: ConnectionConfigFile,
}

impl Default for ServerConfigFile {
    fn default() -> Self {
        Self {
            json_schema: CONFIG_SCHEMA_FILE_NAME.to_owned(),
            connection: ConnectionConfigFile {
                endpoint: ConfigValue::Value("".to_string()),
                headers: BTreeMap::from_iter(vec![(
                    "Authorization".to_owned(),
                    Header {
                        value: ConfigValue::ValueFromEnv(
                            "GRAPHQL_ENDPOINT_AUTHORIZATION".to_string(),
                        ),
                    },
                )]),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ConnectionConfigFile {
    pub endpoint: ConfigValue,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub headers: BTreeMap<String, Header<ConfigValue>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Header<T> {
    pub value: T,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub enum ConfigValue {
    #[serde(rename = "value")]
    Value(String),
    #[serde(rename = "valueFromEnv")]
    ValueFromEnv(String),
}
