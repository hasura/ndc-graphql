use super::state::ServerState;
use crate::query_builder::build_query_document;
use common::{
    client::{execute_graphql, GraphQLRequest},
    config::{
        schema::{ObjectFieldDefinition, TypeDef},
        ServerConfig,
    },
};
use indexmap::IndexMap;
use ndc_sdk::{
    connector::QueryError,
    models::{self, FieldName},
};
use std::collections::{BTreeMap, BTreeSet};
use tracing::{Instrument, Level};

pub async fn handle_query_explain(
    configuration: &ServerConfig,
    _state: &ServerState,
    request: models::QueryRequest,
) -> Result<models::ExplainResponse, QueryError> {
    let operation = tracing::info_span!("Build Query Document", internal.visibility = "user")
        .in_scope(|| build_query_document(&request, configuration))
        .map_err(|err| QueryError::new_invalid_request(&err))?;

    let query =
        serde_json::to_string_pretty(&GraphQLRequest::new(&operation.query, &operation.variables))
            .map_err(|err| QueryError::new_invalid_request(&err))?;

    let details = BTreeMap::from_iter(vec![
        ("SQL Query".to_string(), operation.query),
        ("Execution Plan".to_string(), query),
        (
            "Headers".to_string(),
            serde_json::to_string(&operation.headers).expect("should convert headers to json"),
        ),
    ]);

    Ok(models::ExplainResponse { details })
}

pub async fn handle_query(
    configuration: &ServerConfig,
    state: &ServerState,
    request: models::QueryRequest,
) -> Result<models::QueryResponse, QueryError> {
    #[cfg(debug_assertions)]
    {
        // this block only present in debug builds, to avoid leaking sensitive information
        let request_string =
            serde_json::to_string(&request).map_err(|err| QueryError::new_invalid_request(&err))?;
        tracing::event!(Level::DEBUG, "Incoming IR" = request_string);
    }

    let operation = tracing::info_span!("Build Query Document", internal.visibility = "user")
        .in_scope(|| build_query_document(&request, configuration))?;

    let client = state
        .client(configuration)
        .await
        .map_err(|err| QueryError::new_invalid_request(&err))?;

    let execution_span = tracing::info_span!("Execute GraphQL Query", internal.visibility = "user");

    let (headers, response) = execute_graphql::<IndexMap<FieldName, models::RowFieldValue>>(
        &operation.query,
        operation.variables,
        &configuration.connection.endpoint,
        &operation.headers,
        &client,
        &configuration.response.forward_headers,
    )
    .instrument(execution_span)
    .await
    .map_err(|err| QueryError::new_invalid_request(&err))?;

    tracing::info_span!("Process Response").in_scope(|| {
        if let Some(errors) = response.errors {
            Err(QueryError::new_unprocessable_content(&errors[0].message)
                .with_details(serde_json::json!({ "errors": errors })))
        } else if let Some(data) = response.data {
            let data = normalize_query_data(data, &request, configuration)?;
            let forward_response_headers = !configuration.response.forward_headers.is_empty();

            let row = if forward_response_headers {
                let headers = serde_json::to_value(headers)
                    .map_err(|err| QueryError::new_unprocessable_content(&err))?;
                let data = serde_json::to_value(data)
                    .map_err(|err| QueryError::new_unprocessable_content(&err))?;

                IndexMap::from_iter(vec![
                    (
                        configuration.response.headers_field.to_string().into(),
                        models::RowFieldValue(headers),
                    ),
                    (
                        configuration.response.response_field.to_string().into(),
                        models::RowFieldValue(data),
                    ),
                ])
            } else {
                data
            };

            Ok(models::QueryResponse(vec![models::RowSet {
                groups: None,
                aggregates: None,
                rows: Some(vec![row]),
            }]))
        } else {
            Err(QueryError::new_unprocessable_content(
                &"No data or errors in response",
            ))
        }
    })
}

fn normalize_query_data(
    data: IndexMap<FieldName, models::RowFieldValue>,
    request: &models::QueryRequest,
    configuration: &ServerConfig,
) -> Result<IndexMap<FieldName, models::RowFieldValue>, QueryError> {
    let root_field = request
        .query
        .fields
        .as_ref()
        .and_then(|fields| fields.get("__value"))
        .ok_or_else(|| {
            QueryError::new_invalid_request(&"Misshapen request: no fields for query")
        })?;

    let (root_nested_fields, root_field_definition) = match root_field {
        models::Field::Column { column, fields, .. } if column.inner() == "__value" => {
            let root_field_definition = configuration
                .schema
                .query_fields
                .get(&request.collection)
                .ok_or_else(|| {
                    QueryError::new_invalid_request(&format!(
                        "Field {} not found in Query type",
                        request.collection
                    ))
                })?;
            (fields.as_ref(), root_field_definition)
        }
        models::Field::Column { column, .. } => {
            return Err(QueryError::new_invalid_request(&format!(
                "Expected field with key __value, got {column}"
            )));
        }
        models::Field::Relationship { .. } => {
            return Err(QueryError::new_invalid_request(
                &"Misshapen request: root __value field cannot be a relationship",
            ));
        }
    };

    let mut normalized = IndexMap::with_capacity(data.len());
    for (field_name, row_field_value) in data {
        if is_root_value_alias(field_name.inner()) {
            let normalized_value = normalize_field_value(
                row_field_value.0,
                root_nested_fields,
                root_field_definition,
                &configuration.schema.definitions,
            )?;
            normalized.insert(field_name, models::RowFieldValue(normalized_value));
        } else {
            normalized.insert(field_name, row_field_value);
        }
    }

    Ok(normalized)
}

fn is_root_value_alias(alias: &str) -> bool {
    if alias == "__value" {
        return true;
    }

    alias
        .strip_prefix('q')
        .and_then(|tail| tail.strip_suffix("__value"))
        .is_some_and(|index| !index.is_empty() && index.chars().all(|ch| ch.is_ascii_digit()))
}

fn normalize_field_value(
    value: serde_json::Value,
    nested_field: Option<&models::NestedField>,
    field_definition: &ObjectFieldDefinition,
    definitions: &BTreeMap<models::TypeName, TypeDef>,
) -> Result<serde_json::Value, QueryError> {
    let Some(nested_field) = nested_field else {
        return Ok(value);
    };

    if value.is_null() {
        return Ok(value);
    }

    match nested_field {
        models::NestedField::Array(array) => match value {
            serde_json::Value::Array(values) => {
                let mut normalized_values = Vec::with_capacity(values.len());
                for value in values {
                    normalized_values.push(normalize_field_value(
                        value,
                        Some(&array.fields),
                        field_definition,
                        definitions,
                    )?);
                }
                Ok(serde_json::Value::Array(normalized_values))
            }
            other => Ok(other),
        },
        models::NestedField::Object(object) => {
            normalize_object_like_value(value, &object.fields, field_definition, definitions)
        }
        models::NestedField::Collection(_) => Ok(value),
    }
}

fn normalize_object_like_value(
    value: serde_json::Value,
    requested_fields: &IndexMap<FieldName, models::Field>,
    field_definition: &ObjectFieldDefinition,
    definitions: &BTreeMap<models::TypeName, TypeDef>,
) -> Result<serde_json::Value, QueryError> {
    let object_name = field_definition.r#type.name();
    match definitions.get(&object_name) {
        Some(TypeDef::Object { fields, .. }) => {
            normalize_object_value(value, requested_fields, fields, definitions)
        }
        Some(TypeDef::Interface { possible_types, .. }) => {
            let common_fields = match definitions.get(&object_name) {
                Some(TypeDef::Interface { fields, .. }) => Some(fields),
                _ => None,
            };
            normalize_polymorphic_value(
                value,
                requested_fields,
                possible_types,
                common_fields,
                definitions,
            )
        }
        Some(TypeDef::Union { members, .. }) => {
            normalize_polymorphic_value(value, requested_fields, members, None, definitions)
        }
        Some(_) | None => Ok(value),
    }
}

fn normalize_object_value(
    value: serde_json::Value,
    requested_fields: &IndexMap<FieldName, models::Field>,
    object_fields: &BTreeMap<FieldName, ObjectFieldDefinition>,
    definitions: &BTreeMap<models::TypeName, TypeDef>,
) -> Result<serde_json::Value, QueryError> {
    let source = match value {
        serde_json::Value::Object(source) => source,
        other => return Ok(other),
    };

    let normalized =
        normalize_object_from_source(&source, requested_fields, object_fields, definitions)?;
    Ok(serde_json::Value::Object(normalized))
}

fn normalize_object_from_source(
    source: &serde_json::Map<String, serde_json::Value>,
    requested_fields: &IndexMap<FieldName, models::Field>,
    object_fields: &BTreeMap<FieldName, ObjectFieldDefinition>,
    definitions: &BTreeMap<models::TypeName, TypeDef>,
) -> Result<serde_json::Map<String, serde_json::Value>, QueryError> {
    let mut normalized = serde_json::Map::new();

    for (alias, field) in requested_fields {
        let (column, nested_field) = match field {
            models::Field::Column { column, fields, .. } => (column, fields.as_ref()),
            models::Field::Relationship { .. } => continue,
        };

        let field_definition = object_fields.get(column).ok_or_else(|| {
            QueryError::new_invalid_request(&format!(
                "Field {column} is not defined in object type for response shaping"
            ))
        })?;

        if let Some(value) = lookup_requested_value(source, alias, column) {
            let normalized_value =
                normalize_field_value(value, nested_field, field_definition, definitions)?;
            normalized.insert(alias.to_string(), normalized_value);
        }
    }

    Ok(normalized)
}

fn normalize_polymorphic_value(
    value: serde_json::Value,
    requested_fields: &IndexMap<FieldName, models::Field>,
    concrete_types: &BTreeSet<models::TypeName>,
    common_fields: Option<&BTreeMap<FieldName, ObjectFieldDefinition>>,
    definitions: &BTreeMap<models::TypeName, TypeDef>,
) -> Result<serde_json::Value, QueryError> {
    let source = match value {
        serde_json::Value::Object(source) => source,
        other => return Ok(other),
    };

    let runtime_type_name = runtime_type_name(&source, requested_fields);
    let mut normalized = serde_json::Map::new();

    for (alias, field) in requested_fields {
        let (column, nested_field) = match field {
            models::Field::Column { column, fields, .. } => (column, fields.as_ref()),
            models::Field::Relationship { .. } => continue,
        };

        if column.inner() == "__typename" {
            if let Some(value) = lookup_requested_value(&source, alias, column) {
                normalized.insert(alias.to_string(), value);
            }
            continue;
        }

        let Some(concrete_type_name) = column.inner().strip_prefix("on_") else {
            if let (Some(field_definition), Some(value)) = (
                common_fields.and_then(|fields| fields.get(column)),
                lookup_requested_value(&source, alias, column),
            ) {
                let normalized_value =
                    normalize_field_value(value, nested_field, field_definition, definitions)?;
                normalized.insert(alias.to_string(), normalized_value);
            }
            continue;
        };

        let concrete_type: models::TypeName = concrete_type_name.to_owned().into();
        if !concrete_types.contains(&concrete_type) {
            if let Some(value) = lookup_requested_value(&source, alias, column) {
                normalized.insert(alias.to_string(), value);
            }
            continue;
        }

        if runtime_type_name != Some(concrete_type_name) {
            normalized.insert(alias.to_string(), serde_json::Value::Null);
            continue;
        }

        let requested_subfields = nested_field.and_then(underlying_fields).ok_or_else(|| {
            QueryError::new_invalid_request(&format!(
                "Field {column} requires nested object fields for polymorphic response shaping"
            ))
        })?;

        let concrete_object_fields = match definitions.get(&concrete_type) {
            Some(TypeDef::Object { fields, .. }) => fields,
            Some(_) => {
                return Err(QueryError::new_invalid_request(&format!(
                    "Type {concrete_type} is not an object type for polymorphic response shaping"
                )));
            }
            None => {
                return Err(QueryError::new_invalid_request(&format!(
                    "Type {concrete_type} not found for polymorphic response shaping"
                )));
            }
        };
        let concrete_variant_fields =
            interface_exclusive_fields(concrete_object_fields, common_fields);

        let variant_object = normalize_object_from_source(
            &source,
            requested_subfields,
            &concrete_variant_fields,
            definitions,
        )?;
        normalized.insert(alias.to_string(), serde_json::Value::Object(variant_object));
    }

    Ok(serde_json::Value::Object(normalized))
}

fn underlying_fields(
    nested_field: &models::NestedField,
) -> Option<&IndexMap<FieldName, models::Field>> {
    match nested_field {
        models::NestedField::Object(object) => Some(&object.fields),
        models::NestedField::Array(array) => underlying_fields(&array.fields),
        models::NestedField::Collection(_) => None,
    }
}

fn lookup_requested_value(
    source: &serde_json::Map<String, serde_json::Value>,
    alias: &FieldName,
    column: &FieldName,
) -> Option<serde_json::Value> {
    source
        .get(alias.inner().as_str())
        .cloned()
        .or_else(|| source.get(column.inner().as_str()).cloned())
}

fn runtime_type_name<'a>(
    source: &'a serde_json::Map<String, serde_json::Value>,
    requested_fields: &IndexMap<FieldName, models::Field>,
) -> Option<&'a str> {
    for (alias, field) in requested_fields {
        let models::Field::Column { column, .. } = field else {
            continue;
        };
        if column.inner() == "__typename" {
            if let Some(name) = source
                .get(alias.inner().as_str())
                .or_else(|| source.get(column.inner().as_str()))
                .and_then(serde_json::Value::as_str)
            {
                return Some(name);
            }
        }
    }

    source.get("__typename").and_then(serde_json::Value::as_str)
}

fn interface_exclusive_fields(
    concrete_fields: &BTreeMap<FieldName, ObjectFieldDefinition>,
    common_fields: Option<&BTreeMap<FieldName, ObjectFieldDefinition>>,
) -> BTreeMap<FieldName, ObjectFieldDefinition> {
    match common_fields {
        Some(common_fields) => concrete_fields
            .iter()
            .filter(|(field_name, _)| !common_fields.contains_key(*field_name))
            .map(|(field_name, field_definition)| {
                (field_name.to_owned(), field_definition.to_owned())
            })
            .collect(),
        None => concrete_fields
            .iter()
            .map(|(field_name, field_definition)| {
                (field_name.to_owned(), field_definition.to_owned())
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_query_data;
    use crate::connector::setup::GraphQLConnectorSetup;
    use common::config::ServerConfig;
    use indexmap::IndexMap;
    use ndc_sdk::models::{self, FieldName};
    use serde_json::json;
    use std::{
        collections::{BTreeMap, HashMap},
        fs,
        path::PathBuf,
    };

    async fn read_configuration(config: &str) -> ServerConfig {
        let configuration_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join(config)
            .join("configuration");
        let env = HashMap::from_iter(vec![
            ("GRAPHQL_ENDPOINT".to_owned(), String::new()),
            ("GRAPHQL_ENDPOINT_SECRET".to_owned(), String::new()),
        ]);
        GraphQLConnectorSetup::new(env)
            .read_configuration(configuration_dir)
            .await
            .expect("Should read test configuration")
    }

    fn read_polymorphic_request() -> models::QueryRequest {
        let request_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("config-4")
            .join("queries")
            .join("01_polymorphic_query.request.json");
        let request = fs::read_to_string(request_path).expect("Should read polymorphic request");
        serde_json::from_str(&request).expect("Should parse request")
    }

    #[tokio::test]
    async fn normalizes_polymorphic_response_shape() {
        let configuration = read_configuration("config-4").await;
        let request = read_polymorphic_request();

        let data = IndexMap::from([(
            FieldName::from("__value"),
            models::RowFieldValue(json!({
                "id": "acct-1",
                "relationship": {
                    "__typename": "PersonRelationship",
                    "relId": "rel-1",
                    "personName": "Leia"
                },
                "altRelationship": {
                    "__typename": "OrganizationRelationship",
                    "relId": "alt-rel"
                }
            })),
        )]);

        let normalized = normalize_query_data(data, &request, &configuration)
            .expect("Should normalize polymorphic response");
        let normalized_json = serde_json::to_value(normalized).expect("Should serialize response");

        assert_eq!(
            normalized_json,
            json!({
                "__value": {
                    "id": "acct-1",
                    "relationship": {
                        "__typename": "PersonRelationship",
                        "relId": "rel-1",
                        "on_PersonRelationship": {
                            "personName": "Leia"
                        },
                        "on_OrganizationRelationship": null
                    },
                    "altRelationship": {
                        "__typename": "OrganizationRelationship",
                        "on_PersonRelationship": null
                    }
                }
            })
        );
    }

    #[tokio::test]
    async fn normalizes_polymorphic_response_shape_with_alias() {
        let configuration = read_configuration("config-4").await;
        let mut request = read_polymorphic_request();
        rename_polymorphic_alias(
            &mut request,
            "relationship",
            "on_PersonRelationship",
            "onPerson",
        );
        rename_polymorphic_alias(&mut request, "relationship", "__typename", "typename");

        let data = IndexMap::from([(
            FieldName::from("__value"),
            models::RowFieldValue(json!({
                "id": "acct-1",
                "relationship": {
                    "typename": "PersonRelationship",
                    "relId": "rel-1",
                    "personName": "Leia"
                },
                "altRelationship": {
                    "__typename": "PersonRelationship",
                    "relId": "alt-rel"
                }
            })),
        )]);

        let normalized = normalize_query_data(data, &request, &configuration)
            .expect("Should normalize aliased polymorphic response");
        let normalized_json = serde_json::to_value(normalized).expect("Should serialize response");

        assert_eq!(
            normalized_json,
            json!({
                "__value": {
                    "id": "acct-1",
                    "relationship": {
                        "typename": "PersonRelationship",
                        "relId": "rel-1",
                        "onPerson": {
                            "personName": "Leia"
                        },
                        "on_OrganizationRelationship": null
                    },
                    "altRelationship": {
                        "__typename": "PersonRelationship",
                        "on_PersonRelationship": {
                            "relId": "alt-rel"
                        }
                    }
                }
            })
        );
    }

    #[tokio::test]
    async fn normalizes_interface_common_field() {
        let configuration = read_configuration("config-4").await;
        let mut request = read_polymorphic_request();
        replace_relationship_selection_with_common_field(&mut request, "relId");

        let data = IndexMap::from([(
            FieldName::from("__value"),
            models::RowFieldValue(json!({
                "id": "acct-1",
                "relationship": {
                    "relId": "rel-1"
                }
            })),
        )]);

        let normalized = normalize_query_data(data, &request, &configuration)
            .expect("Should normalize interface common field");
        let normalized_json = serde_json::to_value(normalized).expect("Should serialize response");

        assert_eq!(
            normalized_json,
            json!({
                "__value": {
                    "id": "acct-1",
                    "relationship": {
                        "relId": "rel-1"
                    }
                }
            })
        );
    }

    fn rename_polymorphic_alias(
        request: &mut models::QueryRequest,
        relation_field: &str,
        old_alias: &str,
        new_alias: &str,
    ) {
        let root_field = request
            .query
            .fields
            .as_mut()
            .and_then(|fields| fields.get_mut("__value"))
            .expect("request should contain __value");

        let models::Field::Column {
            fields: Some(models::NestedField::Object(root_object)),
            ..
        } = root_field
        else {
            panic!("__value should contain an object selection");
        };

        let relationship_field = root_object
            .fields
            .get_mut(relation_field)
            .expect("request should contain relationship field");

        let models::Field::Column {
            fields: Some(models::NestedField::Object(relationship_object)),
            ..
        } = relationship_field
        else {
            panic!("relationship field should contain an object selection");
        };

        let old_alias: FieldName = old_alias.into();
        let new_alias: FieldName = new_alias.into();
        let field = relationship_object
            .fields
            .swap_remove(&old_alias)
            .expect("request should contain polymorphic field alias");
        relationship_object.fields.insert(new_alias, field);
    }

    fn replace_relationship_selection_with_common_field(
        request: &mut models::QueryRequest,
        field_name: &str,
    ) {
        let root_field = request
            .query
            .fields
            .as_mut()
            .and_then(|fields| fields.get_mut("__value"))
            .expect("request should contain __value");

        let models::Field::Column {
            fields: Some(models::NestedField::Object(root_object)),
            ..
        } = root_field
        else {
            panic!("__value should contain an object selection");
        };

        let relationship_field = root_object
            .fields
            .get_mut("relationship")
            .expect("request should contain relationship field");

        let models::Field::Column {
            fields: Some(models::NestedField::Object(relationship_object)),
            ..
        } = relationship_field
        else {
            panic!("relationship field should contain an object selection");
        };

        relationship_object.fields = IndexMap::from([(
            FieldName::from(field_name),
            models::Field::Column {
                column: field_name.into(),
                fields: None,
                arguments: BTreeMap::new(),
            },
        )]);
    }
}
