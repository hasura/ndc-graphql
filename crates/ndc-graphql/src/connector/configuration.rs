use std::{env, iter::once, path::PathBuf};

use common::{
    config::{ConnectionConfig, ServerConfig},
    config_file::{ConfigValue, Header, ServerConfigFile, CONFIG_FILE_NAME, SCHEMA_FILE_NAME},
};
use graphql_parser::parse_schema;
use ndc_sdk::connector::{InvalidNode, InvalidNodes, KeyOrIndex, LocatedError, ParseError};
use tokio::fs;

pub async fn read_configuration(context_path: &PathBuf) -> Result<ServerConfig, ParseError> {
    let config_file_path = context_path.join(CONFIG_FILE_NAME);
    let config_file = fs::read_to_string(&config_file_path)
        .await
        .map_err(|err| ParseError::IoError(err))?;
    let config_file: ServerConfigFile = serde_json::from_str(&config_file).map_err(|err| {
        ParseError::ParseError(LocatedError {
            file_path: config_file_path.clone(),
            line: err.line(),
            column: err.column(),
            message: err.to_string(),
        })
    })?;

    let schema_file_path = context_path.join(SCHEMA_FILE_NAME);
    let schema_string = fs::read_to_string(schema_file_path)
        .await
        .map_err(|err| ParseError::IoError(err))?;
    // temporary: we'd like to keep a parsed schema as config
    // lifetimes of the graphql parser library are making this difficult
    // for now, parse the schema for validation, but discard the value.
    let _schema_ast = parse_schema(&schema_string)
        .map_err(|err| {
            ParseError::ParseError(LocatedError {
                file_path: config_file_path.clone(),
                line: 0,
                column: 0,
                message: err.to_string(),
            })
        })?
        .into_static();

    let config = ServerConfig {
        schema_string,
        connection: ConnectionConfig {
            endpoint: read_config_value(
                &config_file_path,
                &["connection", "endpoint"],
                config_file.connection.endpoint,
            )?,
            headers: config_file
                .connection
                .headers
                .into_iter()
                .map(|(header_name, header_value)| {
                    let value = read_config_value(
                        &config_file_path,
                        &["connection", "headers", &header_name, "value"],
                        header_value.value,
                    )?;
                    Ok((header_name, Header { value }))
                })
                .collect::<Result<_, ParseError>>()?,
        },
    };

    Ok(config)
}

fn read_config_value(
    file_path: &PathBuf,
    node_path: &[&str],
    value: ConfigValue,
) -> Result<String, ParseError> {
    match value {
        ConfigValue::Value(v) => Ok(v),
        ConfigValue::ValueFromEnv(e) => Ok(env::var(&e).map_err(|err| {
            ParseError::ValidateError(InvalidNodes(vec![InvalidNode {
                file_path: file_path.to_owned(),
                node_path: node_path
                    .iter()
                    .map(|s| KeyOrIndex::Key((*s).to_owned()))
                    .chain(once(KeyOrIndex::Key("valueFromEnv".to_owned())))
                    .collect(),
                message: format!("Error reading env var {}: {}", e, err),
            }]))
        })?),
    }
}
