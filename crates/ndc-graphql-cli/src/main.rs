use clap::{Parser, Subcommand, ValueEnum};
use common::{
    capabilities_response::capabilities_response,
    config::{
        config_file::{
            ConfigValue, ServerConfigFile, CONFIG_FILE_NAME, CONFIG_SCHEMA_FILE_NAME,
            SCHEMA_FILE_NAME,
        },
        schema::SchemaDefinition,
        ConnectionConfig,
    },
    schema_response::schema_response,
};
use graphql::{execute_graphql_introspection, schema_from_introspection};
use graphql_parser::schema;
use ndc_graphql_cli::graphql;
use ndc_sdk::models;
use schemars::schema_for;
use serde::Serialize;
use std::{
    env,
    error::Error,
    path::{Path, PathBuf},
};
use tokio::fs;

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
    PrintSchemaAndCapabilities {},
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

#[derive(Serialize)]
struct SchemaAndCapabilities {
    schema: models::SchemaResponse,
    capabilities: models::CapabilitiesResponse,
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
            let (config_file, schema_document) = update_config(&context_path).await?;

            validate_config(config_file, schema_document).await?;
        }
        Command::Validate {} => {
            let config_file = read_config_file(&context_path)
                .await?
                .ok_or_else(|| format!("Could not find {CONFIG_FILE_NAME}"))?;
            let schema_document = read_schema_file(&context_path)
                .await?
                .ok_or_else(|| format!("Could not find {SCHEMA_FILE_NAME}"))?;

            validate_config(config_file, schema_document).await?;
        }
        Command::Watch {} => {
            todo!("implement watch command")
        }
        Command::PrintSchemaAndCapabilities {} => {
            let config_file = read_config_file(&context_path)
                .await?
                .ok_or_else(|| format!("Could not find {CONFIG_FILE_NAME}"))?;
            let schema_document = read_schema_file(&context_path)
                .await?
                .ok_or_else(|| format!("Could not find {SCHEMA_FILE_NAME}"))?;

            let request_config = config_file.request.into();
            let response_config = config_file.response.into();

            let schema =
                SchemaDefinition::new(&schema_document, &request_config, &response_config)?;

            let schema_and_capabilities = SchemaAndCapabilities {
                schema: schema_response(&schema, &request_config, &response_config),
                capabilities: capabilities_response(),
            };

            println!(
                "{}",
                serde_json::to_string(&schema_and_capabilities)
                    .expect("Schema and capabilities should serialize to JSON")
            )
        }
    }

    Ok(())
}

async fn write_config_file(
    context_path: &Path,
    config: &ServerConfigFile,
) -> Result<(), Box<dyn Error>> {
    let config_file_path = context_path.join(CONFIG_FILE_NAME);
    let config_file = serde_json::to_string_pretty(&config)?;
    fs::write(config_file_path, config_file).await?;
    Ok(())
}
async fn write_schema_file(
    context_path: &Path,
    schema: &graphql_parser::schema::Document<'_, String>,
) -> Result<(), Box<dyn Error>> {
    let schema_file_path = context_path.join(SCHEMA_FILE_NAME);
    let schema_file = schema.to_string();
    fs::write(schema_file_path, schema_file).await?;
    Ok(())
}
async fn write_config_schema_file(context_path: &Path) -> Result<(), Box<dyn Error>> {
    let config_schema_file_path = context_path.join(CONFIG_SCHEMA_FILE_NAME);
    let config_schema_file = schema_for!(ServerConfigFile);
    fs::write(
        &config_schema_file_path,
        serde_json::to_string_pretty(&config_schema_file)?,
    )
    .await?;
    Ok(())
}

async fn read_config_file(context_path: &Path) -> Result<Option<ServerConfigFile>, Box<dyn Error>> {
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
    context_path: &Path,
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

async fn validate_config(
    config_file: ServerConfigFile,
    schema_document: graphql_parser::schema::Document<'_, String>,
) -> Result<(), Box<dyn Error>> {
    let request_config = config_file.request.into();
    let response_config = config_file.response.into();

    let _schema = SchemaDefinition::new(&schema_document, &request_config, &response_config)?;

    Ok(())
}

async fn update_config(
    context_path: &Path,
) -> Result<
    (
        ServerConfigFile,
        graphql_parser::schema::Document<'static, String>,
    ),
    Box<dyn Error>,
> {
    let config_file = match read_config_file(context_path).await? {
        Some(config) => Ok(config),
        None => {
            println!("Configuration file {CONFIG_FILE_NAME} missing, initializing configuration directory.");
            write_config_schema_file(context_path).await?;
            write_config_file(context_path, &ServerConfigFile::default()).await?;
            Err::<_, String>("Configuration file could not be found, created a new one. Please fill in connection information before trying again.".into())
        }
    }?;

    // CLI uses the introspection connection
    let connection_file = &config_file.introspection;

    let connection = ConnectionConfig {
        endpoint: read_config_value(&connection_file.endpoint)?,
        headers: connection_file
            .headers
            .iter()
            .map(|(header_name, header_value)| {
                Ok((header_name.to_owned(), read_config_value(header_value)?))
            })
            .collect::<Result<_, std::env::VarError>>()?,
    };

    let response = execute_graphql_introspection(&connection).await?;

    if let Some(errors) = response.errors {
        return Err(format!("Introspection error: {}", serde_json::to_string(&errors)?).into());
    }

    let introspection = response
        .data
        .expect("Introspection without error should have data");

    let schema_document = schema_from_introspection(introspection);

    write_schema_file(context_path, &schema_document).await?;
    write_config_schema_file(context_path).await?;

    Ok((config_file, schema_document))
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
    update_config(std::path::Path::new("../../config"))
        .await
        .expect("updating config should work");
}
