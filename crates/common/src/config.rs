use std::collections::BTreeMap;

use crate::{
    config_file::{RequestConfig, ResponseConfig},
    schema::SchemaDefinition,
};

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub connection: ConnectionConfig,
    pub request: RequestConfig<String>,
    pub response: ResponseConfig<String>,
    pub schema: SchemaDefinition,
}

#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    pub endpoint: String,
    pub headers: BTreeMap<String, String>,
}

impl ResponseConfig<String> {
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
