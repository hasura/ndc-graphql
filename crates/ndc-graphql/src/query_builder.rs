use self::{error::QueryBuilderError, operation_variables::OperationVariables};
use common::{
    config::ServerConfig,
    schema::{ObjectFieldDefinition, TypeDef},
};
use glob_match::glob_match;
use graphql_parser::{
    query::{
        Definition, Document, Field, Mutation, OperationDefinition, Query, Selection, SelectionSet,
        Value,
    },
    Pos,
};
use indexmap::IndexMap;
use ndc_sdk::models::{self, Argument, NestedField};
use std::collections::BTreeMap;

pub mod error;
mod operation_variables;

fn pos() -> Pos {
    Pos { line: 0, column: 0 }
}

pub struct Operation {
    pub query: String,
    pub variables: BTreeMap<String, serde_json::Value>,
    pub headers: BTreeMap<String, String>,
}

pub fn build_mutation_document<'a>(
    request: &models::MutationRequest,
    configuration: &ServerConfig,
) -> Result<Operation, QueryBuilderError> {
    let mut variables = OperationVariables::new();

    let mut request_headers = configuration.connection.headers.clone();

    let selection_set = SelectionSet {
        span: (pos(), pos()),
        items: request
            .operations
            .iter()
            .enumerate()
            .map(|(index, operation)| match operation {
                models::MutationOperation::Procedure {
                    name,
                    arguments,
                    fields,
                } => {
                    let alias = format!("procedure_{index}");
                    let field_definition =
                        configuration.schema.query_fields.get(name).ok_or_else(|| {
                            QueryBuilderError::QueryFieldNotFound {
                                field: name.to_owned(),
                            }
                        })?;

                    let (headers, procedure_arguments) =
                        extract_headers(arguments, map_arg, configuration)?;

                    for (name, header) in headers {
                        // if headers are duplicated, the last to be inserted stays
                        // todo: restrict what headers are forwarded here based on config
                        request_headers.insert(name, header.to_string());
                    }

                    selection_set_field(
                        &alias,
                        &name,
                        field_arguments(
                            &procedure_arguments,
                            |v| Ok(v.to_owned()),
                            field_definition,
                            &mut variables,
                        )?,
                        &fields,
                        field_definition,
                        &mut variables,
                        configuration,
                    )
                }
            })
            .collect::<Result<_, _>>()?,
    };

    let (values, variable_definitions) = variables.into_variable_definitions();

    let document: Document<String> = Document {
        definitions: vec![Definition::Operation(OperationDefinition::Mutation(
            Mutation {
                position: pos(),
                name: None,
                variable_definitions,
                directives: vec![],
                selection_set,
            },
        ))],
    };

    Ok(Operation {
        query: document.to_string(),
        variables: values,
        headers: request_headers,
    })
}
pub fn build_query_document<'a>(
    request: &models::QueryRequest,
    configuration: &ServerConfig,
) -> Result<Operation, QueryBuilderError> {
    let mut variables = OperationVariables::new();

    let (headers, request_arguments) =
        extract_headers(&request.arguments, map_query_arg, configuration)?;

    // because all queries are commands, we can expect requests to always have this exact shape
    let selection_set = SelectionSet {
        span: (pos(), pos()),
        items: request
            .query
            .fields
            .as_ref()
            .ok_or_else(|| QueryBuilderError::NoRequesQueryFields)?
            .iter()
            .map(|(alias, field)| {
                let (fields, arguments) = match field {
                    models::Field::Column {
                        column,
                        fields,
                        arguments,
                    } if column == "__value" => Ok((fields, arguments)),
                    models::Field::Column {
                        column,
                        fields: _,
                        arguments: _,
                    } => Err(QueryBuilderError::NotSupported(format!(
                        "Expected field with key __value, got {column}"
                    ))),
                    models::Field::Relationship { .. } => {
                        Err(QueryBuilderError::NotSupported("Relationships".to_string()))
                    }
                }?;

                if !arguments.is_empty() {
                    return Err(QueryBuilderError::Unexpected("Functions arguments should be passed to the collection, not the __value field".to_string()))
                }

                let field_definition = configuration.schema.query_fields.get(&request.collection).ok_or_else(|| QueryBuilderError::QueryFieldNotFound { field: request.collection.to_owned() })?;

                selection_set_field(
                    alias,
                    &request.collection,
                    field_arguments(
                        &request_arguments,
                        map_arg,
                        field_definition,
                        &mut variables,

                    )?,
                    fields,
                    field_definition,
                    &mut variables,
                    configuration,
                )
            })
            .collect::<Result<_, _>>()?,
    };

    let (values, variable_definitions) = variables.into_variable_definitions();

    let document = Document {
        definitions: vec![Definition::Operation(OperationDefinition::Query(Query {
            position: pos(),
            name: None,
            variable_definitions,
            directives: vec![],
            selection_set,
        }))],
    };

    Ok(Operation {
        query: document.to_string(),
        variables: values,
        headers,
    })
}

fn extract_headers<A, M>(
    arguments: &BTreeMap<String, A>,
    map_argument: M,
    configuration: &ServerConfig,
) -> Result<
    (
        BTreeMap<String, String>,
        BTreeMap<String, serde_json::Value>,
    ),
    QueryBuilderError,
>
where
    M: Fn(&A) -> Result<serde_json::Value, QueryBuilderError>,
{
    let mut request_arguments = BTreeMap::new();
    let mut headers = BTreeMap::new();

    for (name, argument) in arguments {
        let value = map_argument(&argument)?;

        if name == &configuration.request.headers_argument {
            match value {
                serde_json::Value::Null
                | serde_json::Value::Bool(_)
                | serde_json::Value::Number(_)
                | serde_json::Value::String(_)
                | serde_json::Value::Array(_) => {
                    return Err(QueryBuilderError::MisshapenHeadersArgument(
                        value.to_owned(),
                    ))
                }
                serde_json::Value::Object(object) => {
                    for (name, value) in object {
                        match value {
                            serde_json::Value::Null
                            | serde_json::Value::Bool(_)
                            | serde_json::Value::Number(_)
                            | serde_json::Value::Array(_)
                            | serde_json::Value::Object(_) => {
                                return Err(QueryBuilderError::MisshapenHeadersArgument(
                                    value.to_owned(),
                                ))
                            }
                            serde_json::Value::String(header) => {
                                for pattern in &configuration.request.forward_headers {
                                    if glob_match(&pattern, &name) {
                                        headers.insert(name, header);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            request_arguments.insert(name.to_owned(), value);
        }
    }

    Ok((headers, request_arguments))
}
fn selection_set_field<'a>(
    alias: &str,
    field_name: &str,
    arguments: Vec<(String, Value<'a, String>)>,
    fields: &Option<NestedField>,
    field_definition: &ObjectFieldDefinition,
    variables: &mut OperationVariables,
    configuration: &ServerConfig,
) -> Result<Selection<'a, String>, QueryBuilderError> {
    let selection_set = match fields.as_ref().map(underlying_fields) {
        Some(fields) => {
            let items = fields
                .iter()
                .map(|(alias, field)| {
                    let (name, fields, arguments) = match field {
                        models::Field::Column {
                            column,
                            fields,
                            arguments,
                        } => (column, fields, arguments),
                        models::Field::Relationship { .. } => {
                            return Err(QueryBuilderError::NotSupported(
                                "Relationships".to_string(),
                            ))
                        }
                    };

                    // subfield selection should only exist on object types
                    let field_definition =
                        match configuration
                            .schema
                            .definitions
                            .get(field_definition.r#type.name())
                        {
                            Some(TypeDef::Object {
                                fields,
                                description: _,
                            }) => fields.get(name).ok_or_else(|| {
                                QueryBuilderError::ObjectFieldNotFound {
                                    object: field_definition.r#type.name().to_owned(),
                                    field: name.to_owned(),
                                }
                            }),
                            Some(_) | None => Err(QueryBuilderError::ObjectTypeNotFound(
                                field_definition.r#type.name().to_owned(),
                            )),
                        }?;

                    selection_set_field(
                        alias,
                        name,
                        field_arguments(arguments, map_query_arg, field_definition, variables)?,
                        fields,
                        field_definition,
                        variables,
                        configuration,
                    )
                })
                .collect::<Result<_, _>>()?;

            SelectionSet {
                span: (pos(), pos()),
                items,
            }
        }
        None => SelectionSet {
            span: (pos(), pos()),
            items: vec![],
        },
    };
    Ok(Selection::Field(Field {
        position: pos(),
        alias: if alias == field_name {
            None
        } else {
            Some(alias.to_owned())
        },
        name: field_name.to_owned(),
        arguments,
        directives: vec![],
        selection_set,
    }))
}
fn field_arguments<'a, A, M>(
    arguments: &BTreeMap<String, A>,
    map_argument: M,
    field_definition: &ObjectFieldDefinition,
    variables: &mut OperationVariables,
) -> Result<Vec<(String, Value<'a, String>)>, QueryBuilderError>
where
    M: Fn(&A) -> Result<serde_json::Value, QueryBuilderError>,
{
    arguments
        .iter()
        .map(|(name, arg)| {
            let input_type = &field_definition.arguments.get(name).unwrap().r#type;

            let value = map_argument(arg)?;

            let value = variables.insert(name, value, input_type);

            Ok((name.to_owned(), value))
        })
        .collect()
}

fn map_query_arg(argument: &models::Argument) -> Result<serde_json::Value, QueryBuilderError> {
    match argument {
        Argument::Variable { name: _ } => {
            Err(QueryBuilderError::NotSupported("Variables".to_owned()))
        }
        Argument::Literal { value } => Ok(value.to_owned()),
    }
}
fn map_arg(argument: &serde_json::Value) -> Result<serde_json::Value, QueryBuilderError> {
    Ok(argument.to_owned())
}

fn underlying_fields(nested_field: &NestedField) -> &IndexMap<String, models::Field> {
    match nested_field {
        NestedField::Object(obj) => &obj.fields,
        NestedField::Array(arr) => underlying_fields(&arr.fields),
    }
}

#[cfg(test)]
mod test {
    use std::collections::BTreeMap;

    use common::{
        config::{ConnectionConfig, ServerConfig},
        config_file::{RequestConfig, ResponseConfig},
        schema::SchemaDefinition,
    };
    use graphql_parser;

    use crate::query_builder::build_query_document;

    #[test]
    fn process_query_into_expected_graphql_string() -> Result<(), Box<dyn std::error::Error>> {
        let schema_string = r#"
          schema {
            query: query_root
          }
          
          
          scalar Int
          
          scalar String
          
          
          type query_root {
            "fetch data from the table: \"test\" using primary key columns"
            test_by_pk(id: Int!): test
          }
          
          "columns and relationships of \"test\""
          type test {
            id: Int!
            name: String!
          }
          
        "#;
        let schema_document = graphql_parser::parse_schema(&schema_string)?;
        let request_config = RequestConfig::default();
        let response_config = ResponseConfig::default();

        let schema = SchemaDefinition::new(&schema_document, &request_config, &response_config)?;
        let configuration = ServerConfig {
            schema,
            request: request_config,
            response: response_config,
            connection: ConnectionConfig {
                endpoint: "".to_string(),
                headers: BTreeMap::new(),
            },
        };
        let request = serde_json::from_value(serde_json::json!({
          "collection": "test_by_pk",
          "query": {
            "fields": {
              "__value": {
                "type": "column",
                "column": "__value",
                "fields": {
                  "type": "object",
                  "fields": {
                    "id": {
                      "type": "column",
                      "column": "id",
                      "fields": null
                    },
                    "name": {
                      "type": "column",
                      "column": "name",
                      "fields": null
                    }
                  }
                }
              }
            }
          },
          "arguments": {
            "_headers": {
                "type": "literal",
                "value": {
                    "Authorization": "Bearer"
                }
            },
            "id": {
              "type": "literal",
              "value": 1
            }
          },
          "collection_relationships": {}
        }))?;

        let operation = build_query_document(&request, &configuration)?;

        let expected_query = r#"
query($arg_1_id: Int!) {
  __value: test_by_pk(id: $arg_1_id) {
    id
    name
  }
}"#;
        let expected_variables =
            BTreeMap::from_iter(vec![("arg_1_id".to_string(), serde_json::json!(1))]);
        let expected_headers =
            BTreeMap::from_iter(vec![("Authorization".to_string(), "Bearer".to_string())]);

        assert_eq!(operation.query.trim(), expected_query.trim());
        assert_eq!(operation.variables, expected_variables);
        assert_eq!(operation.headers, expected_headers);

        Ok(())
    }
}
