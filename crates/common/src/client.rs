use crate::config::ConnectionConfig;
use glob_match::glob_match;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::Serialize;
use std::{
    collections::BTreeMap,
    env,
    error::Error,
    fmt::Debug,
    fs::{self, File},
    io::BufReader,
    path::Path,
};

const CA_CERT_FILE_ENV: &str = "GRAPHQL_CA_CERT_FILE";
const CA_CERT_DIR_ENV: &str = "GRAPHQL_CA_CERT_DIR";
const INSECURE_SKIP_VERIFY_ENV: &str = "GRAPHQL_INSECURE_SKIP_TLS_VERIFY";

fn load_certs_from_file(path: &Path) -> Result<Vec<reqwest::Certificate>, Box<dyn Error>> {
    if !path.is_file() {
        return Err(format!("{} is not a file", path.display()).into());
    }
    tracing::info!("Loading CA certs from file: {}", path.display());
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let certs = rustls_pemfile::certs(&mut reader).collect::<Result<Vec<_>, _>>()?;
    let certs: Vec<_> = certs
        .into_iter()
        .map(|cert| reqwest::Certificate::from_der(cert.as_ref()).map_err(Into::into))
        .collect::<Result<_, Box<dyn Error>>>()?;
    tracing::info!("Loaded {} cert(s) from {}", certs.len(), path.display());
    Ok(certs)
}

fn load_certs_from_dir(dir_path: &str) -> Result<Vec<reqwest::Certificate>, Box<dyn Error>> {
    let path = Path::new(dir_path);
    if !path.is_dir() {
        return Err(format!("{} is not a directory", dir_path).into());
    }
    tracing::info!("Loading CA certs from directory: {}", dir_path);

    let mut all_certs = Vec::new();
    for entry in fs::read_dir(path)?.flatten() {
        let file_path = entry.path();
        let ext = file_path.extension().and_then(|e| e.to_str()).unwrap_or("");
        if !matches!(ext.to_lowercase().as_str(), "pem" | "crt" | "cer") || !file_path.is_file() {
            continue;
        }
        match load_certs_from_file(&file_path) {
            Ok(certs) => all_certs.extend(certs),
            Err(e) => tracing::warn!("Failed to load {}: {}", file_path.display(), e),
        }
    }

    if all_certs.is_empty() {
        return Err(format!("No valid certificates found in {}", dir_path).into());
    }
    tracing::info!(
        "Loaded {} total CA cert(s) from {}",
        all_certs.len(),
        dir_path
    );
    Ok(all_certs)
}

pub fn get_http_client(
    _connection_config: &ConnectionConfig,
) -> Result<reqwest::Client, Box<dyn std::error::Error>> {
    let mut headers = HeaderMap::new();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    let mut builder = reqwest::Client::builder().default_headers(headers);

    // Insecure mode: skip all TLS verification, ignore CA cert settings
    let insecure = env::var(INSECURE_SKIP_VERIFY_ENV)
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false);
    if insecure {
        tracing::warn!(
            "TLS verification DISABLED via {} - insecure!",
            INSECURE_SKIP_VERIFY_ENV
        );
        return Ok(builder.danger_accept_invalid_certs(true).build()?);
    }

    // Load certs from file (if set)
    if let Ok(cert_file) = env::var(CA_CERT_FILE_ENV) {
        for cert in load_certs_from_file(Path::new(&cert_file))? {
            builder = builder.add_root_certificate(cert);
        }
    }

    // Load certs from directory (if set) - can be used together with file
    if let Ok(cert_dir) = env::var(CA_CERT_DIR_ENV) {
        for cert in load_certs_from_dir(&cert_dir)? {
            builder = builder.add_root_certificate(cert);
        }
    }

    Ok(builder.build()?)
}

pub async fn execute_graphql<T: serde::de::DeserializeOwned>(
    query: &str,
    variables: BTreeMap<String, serde_json::Value>,
    endpoint: &str,
    headers: &BTreeMap<String, String>,
    client: &reqwest::Client,
    return_headers: &Vec<String>,
) -> Result<(BTreeMap<String, String>, graphql_client::Response<T>), Box<dyn Error>> {
    let mut request = client.post(endpoint);

    for (header_name, header_value) in headers {
        request = request.header(header_name, header_value);
    }

    let request_body = GraphQLRequest::new(query, &variables);

    let request = request.json(&request_body);

    let response = request.send().await?;
    let headers = response
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            for pattern in return_headers {
                if glob_match(&pattern.to_lowercase(), &name.as_str().to_lowercase()) {
                    return Some((
                        name.to_string(),
                        value.to_str().unwrap_or_default().to_string(),
                    ));
                }
            }
            None
        })
        .collect();

    if response.error_for_status_ref().is_err() {
        return Err(response.text().await?.into());
    }

    let response: graphql_client::Response<T> = response.json().await?;

    Ok((headers, response))
}

#[derive(Debug, Serialize)]
pub struct GraphQLRequest<'a> {
    query: &'a str,
    variables: &'a BTreeMap<String, serde_json::Value>,
}

impl<'a> GraphQLRequest<'a> {
    pub fn new(query: &'a str, variables: &'a BTreeMap<String, serde_json::Value>) -> Self {
        Self { query, variables }
    }
}
