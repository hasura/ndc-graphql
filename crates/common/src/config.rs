use crate::config_file::Header;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub connection: ConnectionConfig,
    pub schema_string: String,
}

#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    pub endpoint: String,
    pub headers: BTreeMap<String, Header<String>>,
}
