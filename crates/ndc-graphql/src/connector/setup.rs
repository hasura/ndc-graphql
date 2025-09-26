use super::{state::ServerState, GraphQLConnector};
use async_trait::async_trait;
use common::config::{
    config_file::{ConfigValue, ServerConfigFile, CONFIG_FILE_NAME, SCHEMA_FILE_NAME},
    schema::SchemaDefinition,
    ConnectionConfig, ServerConfig,
};
use graphql_parser::parse_schema;
use ndc_sdk::connector::{
    self, Connector, ConnectorSetup, InvalidNode, InvalidNodes, KeyOrIndex, LocatedError,
    ParseError,
};
use std::{
    collections::HashMap,
    env,
    iter::once,
    path::{Path, PathBuf},
};
use tokio::fs;

pub struct GraphQLConnectorSetup {
    environment: HashMap<String, String>,
}

#[async_trait]
impl ConnectorSetup for GraphQLConnectorSetup {
    type Connector = GraphQLConnector;

    /// Validate the configuration provided by the user, returning a configuration error or a
    /// validated [`Configuration`].
    ///
    /// The [`ParseError`] type is provided as a convenience to connector authors, to be used on
    /// error.
    async fn parse_configuration(
        &self,
        configuration_dir: &Path,
    ) -> connector::Result<<Self::Connector as Connector>::Configuration> {
        Ok(self.read_configuration(configuration_dir).await?)
    }

    async fn try_init_state(
        &self,
        configuration: &<Self::Connector as Connector>::Configuration,
        _metrics: &mut prometheus::Registry,
    ) -> connector::Result<<Self::Connector as Connector>::State> {
        Ok(ServerState::new(configuration))
    }
}

impl Default for GraphQLConnectorSetup {
    fn default() -> Self {
        Self {
            environment: env::vars().collect(),
        }
    }
}

impl GraphQLConnectorSetup {
    pub fn new(environment: HashMap<String, String>) -> Self {
        Self { environment }
    }
    pub async fn read_configuration(
        &self,
        configuration_dir: impl AsRef<Path> + Send,
    ) -> Result<ServerConfig, ParseError> {
        let config_file_path = configuration_dir.as_ref().join(CONFIG_FILE_NAME);
        let config_file = fs::read_to_string(&config_file_path)
            .await
            .map_err(ParseError::IoError)?;
        let config_file: ServerConfigFile = serde_json::from_str(&config_file).map_err(|err| {
            ParseError::ParseError(LocatedError {
                file_path: config_file_path.clone(),
                line: err.line(),
                column: err.column(),
                message: err.to_string(),
            })
        })?;

        let schema_file_path = configuration_dir.as_ref().join(SCHEMA_FILE_NAME);
        let schema_string = fs::read_to_string(&schema_file_path)
            .await
            .map_err(ParseError::IoError)?;

        let schema_document = parse_schema(&schema_string).map_err(|err| {
            ParseError::ParseError(LocatedError {
                file_path: config_file_path.clone(),
                line: 0,
                column: 0,
                message: err.to_string(),
            })
        })?;

        let request_config = config_file.request.unwrap_or_default().into();
        let response_config = config_file.response.unwrap_or_default().into();

        let schema = SchemaDefinition::new(&schema_document, &request_config, &response_config)
            .map_err(|err| {
                ParseError::ValidateError(InvalidNodes(vec![InvalidNode {
                    file_path: schema_file_path,
                    node_path: vec![],
                    message: err.to_string(),
                }]))
            })?;

        let config = ServerConfig {
            schema,
            connection: ConnectionConfig {
                endpoint: self.read_config_value(
                    &config_file_path,
                    &["connection", "endpoint"],
                    config_file.execution.endpoint,
                )?,
                headers: config_file
                    .execution
                    .headers
                    .into_iter()
                    .map(|(header_name, header_value)| {
                        let value = self.read_config_value(
                            &config_file_path,
                            &["connection", "headers", &header_name, "value"],
                            header_value,
                        )?;
                        Ok((header_name, value))
                    })
                    .collect::<Result<_, ParseError>>()?,
            },
            request: request_config,
            response: response_config,
        };

        Ok(config)
    }

    fn read_config_value(
        &self,
        file_path: &PathBuf,
        node_path: &[&str],
        value: ConfigValue,
    ) -> Result<String, ParseError> {
        match value {
            ConfigValue::Value(v) => Ok(v),
            ConfigValue::ValueFromEnv(e) => {
                Ok(self.environment.get(&e).cloned().ok_or_else(|| {
                    ParseError::ValidateError(InvalidNodes(vec![InvalidNode {
                        file_path: file_path.to_owned(),
                        node_path: node_path
                            .iter()
                            .map(|s| KeyOrIndex::Key((*s).to_owned()))
                            .chain(once(KeyOrIndex::Key("valueFromEnv".to_owned())))
                            .collect(),
                        message: format!("Environment Variable {e} not set"),
                    }]))
                })?)
            }
        }
    }
}
