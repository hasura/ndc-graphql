// These tests call the production query/mutation handlers against a real local
// HTTP endpoint. They do not reimplement response splitting or normalization.
use super::{mutation::handle_mutation, query::handle_query, state::ServerState};
use common::config::{
    schema::SchemaDefinition, ConnectionConfig, RequestConfig, ResponseConfig, ServerConfig,
};
use ndc_sdk::models;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    net::TcpListener,
    thread,
    time::{Duration, Instant},
};

fn config(endpoint: String, forward_headers: bool) -> ServerConfig {
    let document = graphql_parser::parse_schema::<String>(
        r"
        schema { query: Query mutation: Mutation }
        scalar String
        type Query { profilesById(id: String!): [Profile] }
        type Mutation { updateProfile(id: String!): [Profile] }
        type Profile { id: String! name: String }
        ",
    )
    .unwrap();
    let request = RequestConfig::default();
    let mut response = ResponseConfig::default();
    if forward_headers {
        response.forward_headers = vec!["x-request-id".into()];
    }
    let schema = SchemaDefinition::new(&document, &request, &response).unwrap();
    ServerConfig {
        connection: ConnectionConfig {
            endpoint,
            headers: BTreeMap::new(),
        },
        request,
        response,
        schema,
    }
}

fn request(variables: Option<Vec<Value>>) -> models::QueryRequest {
    let mut value = json!({
        "collection": "profilesById",
        "arguments": {"id": {"type": "literal", "value": "00123"}},
        "collection_relationships": {},
        "query": {"fields": {
            "__value": {"type": "column", "column": "__value",
                "fields": {"type": "array", "fields": {"type": "object", "fields": {
                    "id": {"type": "column", "column": "id"},
                    "name": {"type": "column", "column": "name"}
                }}}
            }
        }}
    });
    if let Some(variables) = variables {
        value["variables"] = json!(variables);
        value["arguments"]["id"] = json!({"type": "variable", "name": "id"});
    }
    serde_json::from_value(value).unwrap()
}

// Return a single bounded local mock plus the captured request body. The
// nonblocking accept deadline prevents a failed handler test leaking a thread.
fn upstream(status: u16, body: Value) -> (String, thread::JoinHandle<Value>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/graphql", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let thread = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(Instant::now() < deadline, "handler did not call upstream");
                    thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("{error}"),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut bytes = Vec::new();
        let (header_end, length) = loop {
            let mut buffer = [0; 4096];
            let read = stream.read(&mut buffer).unwrap();
            assert_ne!(read, 0, "incomplete request");
            bytes.extend_from_slice(&buffer[..read]);
            if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..end]);
                let length: usize = headers
                    .lines()
                    .find_map(|line| {
                        let (key, value) = line.split_once(':')?;
                        key.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse().unwrap())
                    })
                    .expect("content-length");
                break (end + 4, length);
            }
        };
        while bytes.len() < header_end + length {
            let mut buffer = [0; 4096];
            let read = stream.read(&mut buffer).unwrap();
            assert_ne!(read, 0, "incomplete body");
            bytes.extend_from_slice(&buffer[..read]);
        }
        let request = serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap();
        let body = body.to_string();
        write!(
            stream,
            "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nX-Request-Id: regression-test\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        request
    });
    (endpoint, thread)
}

async fn run_query(
    variables: Option<Vec<Value>>,
    body: Value,
    forward_headers: bool,
) -> (Value, Value) {
    let (endpoint, server) = upstream(200, body);
    let configuration = config(endpoint, forward_headers);
    let result = handle_query(
        &configuration,
        &ServerState::new(&configuration),
        request(variables),
    )
    .await
    .unwrap();
    let sent = server.join().unwrap();
    (serde_json::to_value(result).unwrap(), sent)
}

#[tokio::test]
async fn direct_list_result_accepts_empty_errors_and_preserves_string_id() {
    let (result, sent) = run_query(
        None,
        json!({"data": {"__value": [{"id": "00123", "name": "A"}]}, "errors": []}),
        false,
    )
    .await;
    assert_eq!(
        result,
        json!([{"rows": [{"__value": [{"id": "00123", "name": "A"}]}]}])
    );
    assert!(sent["query"]
        .as_str()
        .unwrap()
        .contains("__value: profilesById"));
    assert_eq!(sent["variables"]["arg_1_id"], "00123");
}

#[tokio::test]
async fn empty_batch_returns_no_rowsets_without_contacting_upstream() {
    let configuration = config("http://127.0.0.1:1".into(), false);
    let result = handle_query(
        &configuration,
        &ServerState::new(&configuration),
        request(Some(vec![])),
    )
    .await
    .unwrap();
    assert_eq!(serde_json::to_value(result).unwrap(), json!([]));
}

#[tokio::test]
async fn one_variable_still_unaliases_function_result() {
    let (result, sent) = run_query(
        Some(vec![json!({"id": "00123"})]),
        json!({"data": {"q1__value": [{"id": "00123", "name": "A"}]}}),
        false,
    )
    .await;
    assert_eq!(
        result,
        json!([{"rows": [{"__value": [{"id": "00123", "name": "A"}]}]}])
    );
    assert!(sent["query"].as_str().unwrap().contains("q1__value"));
}

#[tokio::test]
async fn batches_preserve_input_order_duplicates_empty_lists_and_nulls() {
    let (result, sent) = run_query(
        Some(vec![
            json!({"id": "00123"}),
            json!({"id": "00456"}),
            json!({"id": "00123"}),
            json!({"id": "unknown"}),
            json!({"id": "null"}),
        ]),
        // Deliberately out of order: result order must come from variable indices.
        json!({"data": {
            "q5__value": null,
            "q4__value": [],
            "q3__value": [{"id": "00123", "name": "A"}],
            "q2__value": [{"id": "00456", "name": "B"}],
            "q1__value": [{"id": "00123", "name": "A"}]
        }}),
        false,
    )
    .await;
    assert_eq!(
        result,
        json!([
            {"rows": [{"__value": [{"id": "00123", "name": "A"}]}]},
            {"rows": [{"__value": [{"id": "00456", "name": "B"}]}]},
            {"rows": [{"__value": [{"id": "00123", "name": "A"}]}]},
            {"rows": [{"__value": []}]},
            {"rows": [{"__value": null}]}
        ])
    );
    for (index, id) in ["00123", "00456", "00123", "unknown", "null"]
        .iter()
        .enumerate()
    {
        assert_eq!(sent["variables"][format!("q{}_arg_1_id", index + 1)], *id);
    }
}

#[tokio::test]
async fn batch_headers_are_wrapped_for_each_parent() {
    let (result, _) = run_query(
        Some(vec![json!({"id": "00123"}), json!({"id": "unknown"})]),
        json!({"data": {"q1__value": [{"id": "00123", "name": "A"}], "q2__value": []}}),
        true,
    )
    .await;
    assert_eq!(
        result,
        json!([
            {"rows": [{"headers": {"x-request-id": "regression-test"}, "response": {
                "__value": [{"id": "00123", "name": "A"}]
            }}]},
            {"rows": [{"headers": {"x-request-id": "regression-test"}, "response": {"__value": []}}]}
        ])
    );
}

#[tokio::test]
async fn missing_alias_is_an_error_but_explicit_null_is_valid() {
    for variables in [None, Some(vec![json!({"id": "00123"})])] {
        let (endpoint, server) = upstream(200, json!({"data": {}}));
        let configuration = config(endpoint, false);
        let error = handle_query(
            &configuration,
            &ServerState::new(&configuration),
            request(variables),
        )
        .await
        .unwrap_err();
        server.join().unwrap();
        assert!(format!("{error:?}").contains("Missing function result alias"));
    }
    let (result, _) = run_query(None, json!({"data": {"__value": null}}), false).await;
    assert_eq!(result, json!([{"rows": [{"__value": null}]}]));
}

#[tokio::test]
async fn upstream_graphql_and_http_errors_remain_errors() {
    for (status, body) in [
        (
            200,
            json!({"data": {"__value": []}, "errors": [{"message": "denied"}]}),
        ),
        (403, json!({"message": "forbidden"})),
        (200, json!({"errors": []})),
    ] {
        let (endpoint, server) = upstream(status, body);
        let configuration = config(endpoint, false);
        assert!(handle_query(
            &configuration,
            &ServerState::new(&configuration),
            request(None),
        )
        .await
        .is_err());
        server.join().unwrap();
    }
}

#[tokio::test]
async fn mutations_accept_empty_errors_without_panicking() {
    let (endpoint, server) = upstream(200, json!({"data": {"procedure_0": []}, "errors": []}));
    let configuration = config(endpoint, false);
    let request: models::MutationRequest = serde_json::from_value(json!({
        "operations": [{"type": "procedure", "name": "updateProfile", "arguments": {"id": "00123"},
            "fields": {"type": "array", "fields": {"type": "object", "fields": {
                "id": {"type": "column", "column": "id"}
            }}}
        }],
        "collection_relationships": {}
    }))
    .unwrap();
    let response = handle_mutation(&configuration, &ServerState::new(&configuration), request)
        .await
        .unwrap();
    server.join().unwrap();
    assert_eq!(
        serde_json::to_value(response).unwrap(),
        json!({"operation_results": [{"type": "procedure", "result": []}]})
    );
}
