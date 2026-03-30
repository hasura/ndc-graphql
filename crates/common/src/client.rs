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
use thiserror::Error;

/// Errors that can occur when executing a GraphQL request against an upstream server.
#[derive(Debug, Error)]
pub enum GraphQLClientError {
    /// Network/connection error (timeout, DNS, TLS, connection refused, etc.)
    #[error("Failed to connect to upstream GraphQL server: {message}")]
    NetworkError {
        message: String,
        endpoint: String,
        #[source]
        source: Option<reqwest::Error>,
    },
    /// Upstream server returned a non-2xx HTTP status code
    #[error("{message}")]
    HttpError {
        status_code: u16,
        message: String,
        body: String,
        endpoint: String,
    },
    /// Failed to parse JSON response from upstream
    #[error("Failed to parse upstream response: {message}")]
    ResponseParseError {
        message: String,
        body: String,
        endpoint: String,
    },
}

impl GraphQLClientError {
    /// Returns a human-readable error message based on the HTTP status code
    fn http_error_message(status_code: u16) -> &'static str {
        match status_code {
            400 => "Upstream server rejected the request",
            401 => "Upstream server authentication failed",
            403 => "Upstream server access forbidden",
            404 => "Upstream GraphQL endpoint not found",
            405 => "HTTP method not allowed by upstream server",
            408 => "Upstream server request timeout",
            429 => "Upstream server rate limit exceeded",
            500 => "Upstream server internal error",
            502 => "Upstream server bad gateway",
            503 => "Upstream server temporarily unavailable",
            504 => "Upstream server gateway timeout",
            _ if (400..500).contains(&status_code) => "Upstream server client error",
            _ if (500..).contains(&status_code) => "Upstream server error",
            _ => "Upstream server returned unexpected status",
        }
    }

    /// Returns the upstream HTTP status code if this is an HTTP error
    pub fn status_code(&self) -> Option<u16> {
        match self {
            Self::HttpError { status_code, .. } => Some(*status_code),
            _ => None,
        }
    }

    /// Returns the endpoint that was called
    pub fn endpoint(&self) -> &str {
        match self {
            Self::NetworkError { endpoint, .. }
            | Self::HttpError { endpoint, .. }
            | Self::ResponseParseError { endpoint, .. } => endpoint,
        }
    }

    /// Returns the response body if available
    pub fn body(&self) -> Option<&str> {
        match self {
            Self::HttpError { body, .. } | Self::ResponseParseError { body, .. } => Some(body),
            Self::NetworkError { .. } => None,
        }
    }

    /// Converts the error to a JSON value suitable for error details
    pub fn to_details(&self) -> serde_json::Value {
        match self {
            Self::NetworkError {
                message, endpoint, ..
            } => serde_json::json!({
                "error_type": "network_error",
                "endpoint": endpoint,
                "message": message,
            }),
            Self::HttpError {
                status_code,
                body,
                endpoint,
                ..
            } => serde_json::json!({
                "error_type": "http_error",
                "upstream_status_code": status_code,
                "upstream_body": body,
                "endpoint": endpoint,
            }),
            Self::ResponseParseError {
                message,
                body,
                endpoint,
            } => serde_json::json!({
                "error_type": "response_parse_error",
                "endpoint": endpoint,
                "message": message,
                "body": body,
            }),
        }
    }
}

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
) -> Result<(BTreeMap<String, String>, graphql_client::Response<T>), GraphQLClientError> {
    let mut request = client.post(endpoint);

    for (header_name, header_value) in headers {
        request = request.header(header_name, header_value);
    }

    let request_body = GraphQLRequest::new(query, &variables);

    let request = request.json(&request_body);

    let response = match request.send().await {
        Ok(resp) => resp,
        Err(err) => {
            let message = err.to_string();
            tracing::error!(
                endpoint = %endpoint,
                error = %err,
                "Failed to connect to upstream GraphQL server"
            );
            return Err(GraphQLClientError::NetworkError {
                message,
                endpoint: endpoint.to_string(),
                source: Some(err),
            });
        }
    };

    // Extract headers before consuming the response
    let response_headers: BTreeMap<String, String> = response
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

    // Extract status code before consuming the response
    let status_code = response.status().as_u16();

    // Check for HTTP errors
    if !response.status().is_success() {
        let body = response.text().await.unwrap_or_default();
        let message = GraphQLClientError::http_error_message(status_code);

        tracing::error!(
            endpoint = %endpoint,
            upstream_status = status_code,
            upstream_body = %body,
            "Upstream GraphQL server returned error"
        );

        return Err(GraphQLClientError::HttpError {
            status_code,
            message: message.to_string(),
            body,
            endpoint: endpoint.to_string(),
        });
    }

    // Parse the JSON response
    let body_text = match response.text().await {
        Ok(text) => text,
        Err(err) => {
            tracing::error!(
                endpoint = %endpoint,
                error = %err,
                "Failed to read response body from upstream"
            );
            return Err(GraphQLClientError::NetworkError {
                message: format!("Failed to read response body: {err}"),
                endpoint: endpoint.to_string(),
                source: Some(err),
            });
        }
    };

    let parsed_response: graphql_client::Response<T> = match serde_json::from_str(&body_text) {
        Ok(resp) => resp,
        Err(err) => {
            tracing::error!(
                endpoint = %endpoint,
                error = %err,
                body = %body_text,
                "Failed to parse JSON response from upstream"
            );
            return Err(GraphQLClientError::ResponseParseError {
                message: err.to_string(),
                body: body_text,
                endpoint: endpoint.to_string(),
            });
        }
    };

    Ok((response_headers, parsed_response))
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
