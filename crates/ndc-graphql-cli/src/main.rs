use std::{env, error::Error, path::PathBuf};

use clap::{Parser, Subcommand, ValueEnum};
use common::{
    config::ConnectionConfig,
    config_file::{
        ConfigValue, ServerConfigFile, CONFIG_FILE_NAME, CONFIG_SCHEMA_FILE_NAME, SCHEMA_FILE_NAME,
    },
    schema::SchemaDefinition,
};
use graphql::{execute_graphql_introspection, schema_from_introspection};
use graphql_parser::schema;
use schemars::schema_for;
use tokio::fs;

mod graphql;

#[derive(Parser)]
struct CliArgs {
    /// The PAT token which can be used to make authenticated calls to Hasura Cloud
    #[arg(long = "ddn-pat", value_name = "PAT", env = "HASURA_PLUGIN_DDN_PAT")]
    ddn_pat: Option<String>,
    /// If the plugins are sending any sort of telemetry back to Hasura, it should be disabled if this is true.
    #[arg(long = "disable-telemetry", env = "HASURA_PLUGIN_DISABLE_TELEMETRY")]
    disable_telemetry: bool,
    /// A UUID for every unique user. Can be used in telemetry
    #[arg(
        long = "instance-id",
        value_name = "ID",
        env = "HASURA_PLUGIN_INSTANCE_ID"
    )]
    instance_id: Option<String>,
    /// A UUID unique to every invocation of Hasura CLI
    #[arg(
        long = "execution-id",
        value_name = "ID",
        env = "HASURA_PLUGIN_EXECUTION_ID"
    )]
    execution_id: Option<String>,
    #[arg(
        long = "log-level",
        value_name = "LEVEL",
        env = "HASURA_PLUGIN_LOG_LEVEL",
        default_value = "info",
        ignore_case = true
    )]
    log_level: LogLevel,
    /// Fully qualified path to the context directory of the connector
    #[arg(
        long = "connector-context-path",
        value_name = "PATH",
        env = "HASURA_PLUGIN_CONNECTOR_CONTEXT_PATH"
    )]
    context_path: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Subcommand)]
enum Command {
    Init {},
    Update {},
    Validate {},
    Watch {},
}

#[derive(Clone, ValueEnum)]
enum LogLevel {
    Panic,
    Fatal,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = CliArgs::parse();

    let context_path = match args.context_path {
        None => env::current_dir()?,
        Some(path) => path,
    };

    match args.command {
        Command::Init {} => {
            write_config_schema_file(&context_path).await?;
            write_config_file(&context_path, &ServerConfigFile::default()).await?;
            println!("Configuration Initialized. Add your endpoint, then introspect your schema to continue.")
        }
        Command::Update {} => {
            update_config(&context_path).await?;
        }
        Command::Validate {} => {
            let config_file = read_config_file(&context_path)
                .await?
                .ok_or_else(|| format!("Could not find {CONFIG_FILE_NAME}"))?;
            let schema_document = read_schema_file(&context_path)
                .await?
                .ok_or_else(|| format!("Could not find {SCHEMA_FILE_NAME}"))?;

            let request_config = config_file
                .request
                .map(|request| request.into())
                .unwrap_or_default();
            let response_config = config_file
                .response
                .map(|response| response.into())
                .unwrap_or_default();

            let _schema =
                SchemaDefinition::new(&schema_document, &request_config, &response_config)?;
        }
        Command::Watch {} => {
            todo!("implement watch command")
        }
    }

    Ok(())
}

async fn write_config_file(
    context_path: &PathBuf,
    config: &ServerConfigFile,
) -> Result<(), Box<dyn Error>> {
    let config_file_path = context_path.join(CONFIG_FILE_NAME);
    let config_file = serde_json::to_string_pretty(&config)?;
    fs::write(config_file_path, config_file).await?;
    Ok(())
}
async fn write_schema_file(
    context_path: &PathBuf,
    schema: &graphql_parser::schema::Document<'_, String>,
) -> Result<(), Box<dyn Error>> {
    let schema_file_path = context_path.join(SCHEMA_FILE_NAME);
    let schema_file = schema.to_string();
    fs::write(schema_file_path, schema_file).await?;
    Ok(())
}
async fn write_config_schema_file(context_path: &PathBuf) -> Result<(), Box<dyn Error>> {
    let config_schema_file_path = context_path.join(CONFIG_SCHEMA_FILE_NAME);
    let config_schema_file = schema_for!(ServerConfigFile);
    fs::write(
        &config_schema_file_path,
        serde_json::to_string_pretty(&config_schema_file)?,
    )
    .await?;
    Ok(())
}

async fn read_config_file(
    context_path: &PathBuf,
) -> Result<Option<ServerConfigFile>, Box<dyn Error>> {
    let file_path = context_path.join(CONFIG_FILE_NAME);
    let config: Option<ServerConfigFile> = match fs::read_to_string(file_path).await {
        Ok(file) => Some(serde_json::from_str(&file)
        .map_err(|err| format!("Error parsing {CONFIG_FILE_NAME}: {err}\n\nDelete {CONFIG_FILE_NAME} to create a fresh file"))),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => Some(Err(format!("Error reading {CONFIG_FILE_NAME}"))),
    }.transpose()?;

    Ok(config)
}

async fn read_schema_file(
    context_path: &PathBuf,
) -> Result<Option<schema::Document<'static, String>>, Box<dyn std::error::Error>> {
    let file_path = context_path.join(SCHEMA_FILE_NAME);
    let config: Option<schema::Document<'static, String>> = match fs::read_to_string(file_path).await {
        Ok(file) => Some(graphql_parser::parse_schema(&file).map(|document| document.into_static())
            .map_err(|err| format!("Error parsing {SCHEMA_FILE_NAME}: {err}\n\nDelete {SCHEMA_FILE_NAME} to create a fresh file"))),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => Some(Err(format!("Error reading {SCHEMA_FILE_NAME}"))),
    }.transpose()?;

    Ok(config)
}

async fn update_config(context_path: &PathBuf) -> Result<(), Box<dyn Error>> {
    let config_file = match read_config_file(&context_path).await? {
        Some(config) => Ok(config),
        None => {
            println!("Configuration file {CONFIG_FILE_NAME} missing, initializing configuration directory.");
            write_config_schema_file(&context_path).await?;
            write_config_file(&context_path, &ServerConfigFile::default()).await?;
            Err::<_, String>("Configuration file could not be found, created a new one. Please fill in connection information before trying again.".into())
        }
    }?;

    // CLI uses the introspection connection if available
    let connection_file = config_file
        .introspection
        .unwrap_or_else(|| config_file.connection);

    let connection = ConnectionConfig {
        endpoint: read_config_value(&connection_file.endpoint)?,
        headers: connection_file
            .headers
            .iter()
            .map(|(header_name, header_value)| {
                Ok((header_name.to_owned(), read_config_value(&header_value)?))
            })
            .collect::<Result<_, std::env::VarError>>()?,
    };

    let response = execute_graphql_introspection(&connection).await?;

    // todo: handle graphql errors!
    let introspection = response.data.expect("Successful introspection");

    let schema = schema_from_introspection(introspection);

    write_schema_file(context_path, &schema).await?;
    write_config_schema_file(&context_path).await?;

    Ok(())
}

fn read_config_value(value: &ConfigValue) -> Result<String, std::env::VarError> {
    match value {
        ConfigValue::Value(v) => Ok(v.to_owned()),
        ConfigValue::ValueFromEnv(e) => Ok(env::var(e)?),
    }
}

#[tokio::test]
#[ignore]
async fn update_configuration_directory() {
    update_config(&std::path::Path::new("../../config").to_path_buf())
        .await
        .expect("updating config should work");
}
