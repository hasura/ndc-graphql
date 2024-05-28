use std::collections::BTreeMap;

use graphql_parser::{
    query::{
        Definition, Document, Field, Mutation, OperationDefinition, Query, Selection, SelectionSet,
        Type, Value,
    },
    schema::{self, InputValue, ObjectType},
    Pos,
};
use indexmap::IndexMap;
use ndc_sdk::models::{self, Argument, NestedField};

use self::{error::QueryBuilderError, operation_variables::OperationVariables};

pub mod error;
mod operation_variables;

fn pos() -> Pos {
    Pos { line: 0, column: 0 }
}

pub fn build_mutation_document<'a>(
    request: &models::MutationRequest,
    schema_document: &'a schema::Document<'a, String>,
) -> Result<(String, BTreeMap<String, serde_json::Value>), QueryBuilderError> {
    let mut variables = OperationVariables::new();

    let schema_type = schema_type(schema_document)?;

    let mutation_type_name = schema_type
        .mutation
        .as_ref()
        .ok_or_else(|| QueryBuilderError::NoMutationType)?;

    let mutation_type = object_type(&mutation_type_name, schema_document)?;

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
                    selection_set_field(
                        &alias,
                        &name,
                        field_arguments(
                            arguments,
                            |v| Ok(v.to_owned()),
                            &name,
                            mutation_type,
                            &mut variables,
                        )?,
                        mutation_type,
                        fields,
                        &mut variables,
                        schema_document,
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

    Ok((document.to_string(), values))
}
pub fn build_query_document<'a>(
    request: &models::QueryRequest,
    schema_document: &'a schema::Document<'a, String>,
) -> Result<(String, BTreeMap<String, serde_json::Value>), QueryBuilderError> {
    // because all queries are commands, we can expect requests to always have this exact shape

    let mut variables = OperationVariables::new();

    let schema_type = schema_type(schema_document)?;

    let query_type_name = schema_type
        .query
        .as_ref()
        .ok_or_else(|| QueryBuilderError::NoQueryType)?;

    let query_type = object_type(&query_type_name, schema_document)?;

    let selection_set = SelectionSet {
        span: (pos(), pos()),
        items: request
            .query
            .fields
            .as_ref()
            .ok_or_else(|| QueryBuilderError::NoRequesQueryFields)?
            .iter()
            .map(|(alias, field)| {
                let fields = match field {
                    models::Field::Column { column, fields } if column == "__value" => Ok(fields),
                    models::Field::Column { column, fields: _ } => {
                        Err(QueryBuilderError::NotSupported(format!(
                            "Expected field with key __value, got {column}"
                        )))
                    }
                    models::Field::Relationship { .. } => {
                        Err(QueryBuilderError::NotSupported("Relationships".to_string()))
                    }
                }?;

                selection_set_field(
                    alias,
                    &request.collection,
                    field_arguments(
                        &request.arguments,
                        map_query_arg,
                        &request.collection,
                        query_type,
                        &mut variables,
                    )?,
                    query_type,
                    fields,
                    &mut variables,
                    schema_document,
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

    Ok((document.to_string(), values))
}
fn selection_set_field<'a>(
    alias: &str,
    name: &str,
    arguments: Vec<(String, Value<'a, String>)>,
    parent_object_type: &'a schema::ObjectType<'a, String>,
    fields: &Option<NestedField>,
    variables: &mut OperationVariables,
    schema_document: &'a schema::Document<'a, String>,
) -> Result<Selection<'a, String>, QueryBuilderError> {
    let selection_set = match fields.as_ref().map(underlying_fields) {
        Some(fields) => {
            // if some, this is an object type
            let field_type = object_field_type(&name, parent_object_type)?;
            let type_name = type_name(&field_type.field_type);
            let field_object_type = object_type(type_name, schema_document)?;

            let items = fields
                .iter()
                .map(|(alias, field)| {
                    let (name, fields) = match field {
                        models::Field::Column { column, fields } => (column, fields),
                        models::Field::Relationship { .. } => {
                            return Err(QueryBuilderError::NotSupported(
                                "Relationships".to_string(),
                            ))
                        }
                    };
                    // todo: support field arguments
                    let arguments = vec![];

                    selection_set_field(
                        alias,
                        name,
                        arguments,
                        field_object_type,
                        fields,
                        variables,
                        schema_document,
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
        alias: if alias == name {
            None
        } else {
            Some(alias.to_owned())
        },
        name: name.to_owned(),
        arguments,
        directives: vec![],
        selection_set,
    }))
}
fn field_arguments<'a, A, M>(
    arguments: &BTreeMap<String, A>,
    map_argument: M,
    field_name: &str,
    object_type: &'a ObjectType<'a, String>,
    variables: &mut OperationVariables<'a>,
) -> Result<Vec<(String, Value<'a, String>)>, QueryBuilderError>
where
    M: Fn(&A) -> Result<serde_json::Value, QueryBuilderError>,
{
    arguments
        .iter()
        .map(|(name, arg)| {
            let input_value = object_field_arg_input_value(&name, field_name, object_type)?;

            let value = map_argument(arg)?;

            let value = variables.insert(name, value, input_value);

            Ok((name.to_owned(), value))
        })
        .collect()
}
fn schema_type<'a>(
    schema_document: &'a schema::Document<'a, String>,
) -> Result<&'a schema::SchemaDefinition<'a, String>, QueryBuilderError> {
    schema_document
        .definitions
        .iter()
        .find_map(|definition| match definition {
            schema::Definition::SchemaDefinition(schema) => Some(schema),
            _ => None,
        })
        .ok_or_else(|| QueryBuilderError::SchemaDefinitionNotFound)
}
fn object_type<'a>(
    type_name: &str,
    schema_document: &'a schema::Document<'a, String>,
) -> Result<&'a schema::ObjectType<'a, String>, QueryBuilderError> {
    schema_document
        .definitions
        .iter()
        .find_map(|definition| match definition {
            schema::Definition::TypeDefinition(schema::TypeDefinition::Object(object))
                if object.name == type_name =>
            {
                Some(object)
            }
            _ => None,
        })
        .ok_or_else(|| QueryBuilderError::ObjectTypeNotFound(type_name.to_owned()))
}
fn object_field_type<'a>(
    field_name: &str,
    object_type: &'a ObjectType<'a, String>,
) -> Result<&'a schema::Field<'a, String>, QueryBuilderError> {
    object_type
        .fields
        .iter()
        .find(|field| field.name == field_name)
        .ok_or_else(|| QueryBuilderError::ObjectFieldNotFound {
            object: object_type.name.to_owned(),
            field: field_name.to_owned(),
        })
}
fn object_field_arg_input_value<'a>(
    argument_name: &str,
    field_name: &str,
    object_type: &'a ObjectType<'a, String>,
) -> Result<&'a InputValue<'a, String>, QueryBuilderError> {
    let field_type = object_field_type(field_name, object_type)?;

    field_type
        .arguments
        .iter()
        .find(|arg| arg.name == argument_name)
        .ok_or_else(|| QueryBuilderError::ArgumentNotFound {
            object: object_type.name.to_owned(),
            field: field_name.to_owned(),
            argument: argument_name.to_owned(),
        })
}

fn type_name<'a>(decorated_type: &'a Type<String>) -> &'a str {
    match decorated_type {
        Type::NamedType(n) => n,
        Type::ListType(l) => type_name(l),
        Type::NonNullType(n) => type_name(n),
    }
}

fn map_query_arg(argument: &models::Argument) -> Result<serde_json::Value, QueryBuilderError> {
    match argument {
        Argument::Variable { name: _ } => {
            Err(QueryBuilderError::NotSupported("Variables".to_owned()))
        }
        Argument::Literal { value } => Ok(value.to_owned()),
    }
}

fn underlying_fields(nested_field: &NestedField) -> &IndexMap<String, models::Field> {
    match nested_field {
        NestedField::Object(obj) => &obj.fields,
        NestedField::Array(arr) => underlying_fields(&arr.fields),
    }
}

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
        "id": {
          "type": "literal",
          "value": 1
        }
      },
      "collection_relationships": {}
    }))?;

    let (document, variables) = build_query_document(&request, &schema_document)?;

    let expected_query = r#"query($arg_1_id: Int!) {
  __value: test_by_pk(id: $arg_1_id) {
    id
    name
  }
}"#;
    let expected_variables =
        BTreeMap::from_iter(vec![("arg_1_id".to_string(), serde_json::json!(1))]);

    assert_eq!(document, expected_query);
    assert_eq!(variables, expected_variables);

    Ok(())
}
