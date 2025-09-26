use self::{error::QueryBuilderError, operation_parameters::OperationParameters};
use common::config::{
    schema::{ObjectFieldDefinition, TypeDef},
    ServerConfig,
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
use ndc_sdk::models::{
    self, Argument, ArgumentName, FieldName, NestedField, TypeName, VariableName,
};
use std::collections::BTreeMap;

pub mod error;
mod operation_parameters;

fn pos() -> Pos {
    Pos { line: 0, column: 0 }
}

pub struct Operation {
    pub query: String,
    pub variables: BTreeMap<String, serde_json::Value>,
    pub headers: BTreeMap<String, String>,
}

pub fn build_mutation_document(
    request: &models::MutationRequest,
    configuration: &ServerConfig,
) -> Result<Operation, QueryBuilderError> {
    // mutations don't have variables, so we use an empty set
    let dummy_variables = BTreeMap::new();
    let mut parameters = OperationParameters::new("");

    let mut request_headers = BTreeMap::new();
    let mut items = vec![];

    let mutation_type_name = configuration
        .schema
        .mutation_type_name
        .as_ref()
        .ok_or(QueryBuilderError::NoMutationType)?;

    for (index, operation) in request.operations.iter().enumerate() {
        match operation {
            models::MutationOperation::Procedure {
                name,
                arguments,
                fields,
            } => {
                let field_name: FieldName = name.to_string().into();
                let alias = format!("procedure_{index}");
                let field_definition =
                    configuration
                        .schema
                        .mutation_fields
                        .get(name)
                        .ok_or_else(|| QueryBuilderError::MutationFieldNotFound {
                            field: name.to_owned(),
                        })?;

                let (headers, procedure_arguments) =
                    extract_headers(arguments, map_arg, configuration, &BTreeMap::new())?;

                // note: duplicate headers get dropped here
                // if there are multiple root fields, preset headers get set here once per field,
                // with the last one persisting.
                // this should not matter as headers should be identical anyways
                request_headers.extend(headers.into_iter());

                let item = selection_set_field(
                    &alias,
                    &field_name,
                    field_arguments(
                        &procedure_arguments,
                        map_arg,
                        field_definition,
                        &mut parameters,
                        &field_name,
                        mutation_type_name,
                        &dummy_variables,
                    )?,
                    fields,
                    field_definition,
                    &mut parameters,
                    configuration,
                    &dummy_variables,
                )?;

                items.push(item);
            }
        }
    }

    let mut request_level_headers =
        extract_headers_from_request_arguments(request.request_arguments.as_ref())?;

    request_headers.append(&mut request_level_headers);

    let selection_set = SelectionSet {
        span: (pos(), pos()),
        items,
    };

    let (values, variable_definitions) = parameters.into_parameter_definitions();

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

pub fn build_query_document(
    request: &models::QueryRequest,
    configuration: &ServerConfig,
) -> Result<Operation, QueryBuilderError> {
    let query_type_name = configuration
        .schema
        .query_type_name
        .as_ref()
        .ok_or(QueryBuilderError::NoQueryType)?;

    let root_field = request
        .query
        .fields
        .as_ref()
        .and_then(|fields| fields.get("__value"))
        .ok_or_else(|| QueryBuilderError::NoRequesQueryFields)?;

    let (subfields, arguments) = match root_field {
        models::Field::Column {
            column,
            fields,
            arguments,
        } if column == &"__value".into() => Ok((fields, arguments)),
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
        return Err(QueryBuilderError::Unexpected(
            "Functions arguments should be passed to the collection, not the __value field"
                .to_string(),
        ));
    }

    let root_field_definition = configuration
        .schema
        .query_fields
        .get(&request.collection)
        .ok_or_else(|| QueryBuilderError::QueryFieldNotFound {
            field: request.collection.clone(),
        })?;

    let (mut headers, items, variable_values, variable_definitions) =
        if let Some(variables) = &request.variables {
            let mut variable_values = BTreeMap::new();
            let mut variable_definitions = vec![];
            let mut items = vec![];
            let mut all_headers = BTreeMap::new();

            for (index, variables) in variables.iter().enumerate() {
                let mut parameters = OperationParameters::new(format!("q{}_", index + 1));

                let (mut headers, request_arguments) =
                    extract_headers(&request.arguments, map_query_arg, configuration, variables)?;

                // note: all_headers is a BTreeMap. Duplicate headers will be discarded here, and the last one will be used
                all_headers.append(&mut headers);

                let item = selection_set_field(
                    &format!("q{}__value", index + 1),
                    &request.collection.to_string().into(),
                    field_arguments(
                        &request_arguments,
                        map_arg,
                        root_field_definition,
                        &mut parameters,
                        &request.collection.to_string().into(),
                        query_type_name,
                        variables,
                    )?,
                    subfields,
                    root_field_definition,
                    &mut parameters,
                    configuration,
                    variables,
                )?;

                let (mut values, mut definitions) = parameters.into_parameter_definitions();

                items.push(item);

                variable_values.append(&mut values);
                variable_definitions.append(&mut definitions);
            }

            (all_headers, items, variable_values, variable_definitions)
        } else {
            let mut parameters = OperationParameters::new("");
            // if the query does not have variables, we use an empty set
            let dummy_variables = BTreeMap::new();

            let (headers, request_arguments) = extract_headers(
                &request.arguments,
                map_query_arg,
                configuration,
                &dummy_variables,
            )?;

            let item = selection_set_field(
                "__value",
                &request.collection.to_string().into(),
                field_arguments(
                    &request_arguments,
                    map_arg,
                    root_field_definition,
                    &mut parameters,
                    &request.collection.to_string().into(),
                    query_type_name,
                    &dummy_variables,
                )?,
                subfields,
                root_field_definition,
                &mut parameters,
                configuration,
                &dummy_variables,
            )?;

            let (variable_values, variable_definitions) = parameters.into_parameter_definitions();

            (headers, vec![item], variable_values, variable_definitions)
        };

    let mut request_level_headers =
        extract_headers_from_request_arguments(request.request_arguments.as_ref())?;

    headers.append(&mut request_level_headers);

    let selection_set = SelectionSet {
        span: (pos(), pos()),
        items,
    };

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
        variables: variable_values,
        headers,
    })
}

type Headers = BTreeMap<String, String>;
type Arguments = BTreeMap<ArgumentName, serde_json::Value>;

/// extract the headers argument if present and applicable
/// returns the headers for this request, including base headers and forwarded headers
fn extract_headers<A, M>(
    arguments: &BTreeMap<ArgumentName, A>,
    map_argument: M,
    configuration: &ServerConfig,
    variables: &BTreeMap<VariableName, serde_json::Value>,
) -> Result<(Headers, Arguments), QueryBuilderError>
where
    M: Fn(
        &A,
        &BTreeMap<VariableName, serde_json::Value>,
    ) -> Result<serde_json::Value, QueryBuilderError>,
{
    let mut request_arguments = BTreeMap::new();
    let mut headers = configuration.connection.headers.clone();

    let patterns = &configuration.request.forward_headers;

    for (name, argument) in arguments {
        let value = map_argument(argument, variables)?;

        if name == &configuration.request.headers_argument {
            match value {
                serde_json::Value::Null
                | serde_json::Value::Bool(_)
                | serde_json::Value::Number(_)
                | serde_json::Value::String(_)
                | serde_json::Value::Array(_) => {
                    return Err(QueryBuilderError::MisshapenHeadersArgument(value.clone()))
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
                                    value.clone(),
                                ))
                            }
                            serde_json::Value::String(header) => {
                                for pattern in patterns {
                                    if glob_match(&pattern.to_lowercase(), &name.to_lowercase()) {
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

#[allow(clippy::too_many_arguments)]
fn selection_set_field<'a>(
    alias: &str,
    field_name: &FieldName,
    arguments: Vec<(String, Value<'a, String>)>,
    fields: &Option<NestedField>,
    field_definition: &ObjectFieldDefinition,
    parameters: &mut OperationParameters,
    configuration: &ServerConfig,
    variables: &BTreeMap<VariableName, serde_json::Value>,
) -> Result<Selection<'a, String>, QueryBuilderError> {
    let selection_set = match fields.as_ref().and_then(underlying_fields) {
        Some(fields) => {
            let items = fields
                .iter()
                .map(|(alias, field)| {
                    let (field_name, fields, arguments) = match field {
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

                    let object_name = field_definition.r#type.name();

                    // subfield selection should only exist on object types
                    let field_definition = match configuration.schema.definitions.get(&object_name)
                    {
                        Some(TypeDef::Object {
                            fields,
                            description: _,
                        }) => fields.get(field_name).ok_or_else(|| {
                            QueryBuilderError::ObjectFieldNotFound {
                                object: field_definition.r#type.name(),
                                field: field_name.clone(),
                            }
                        }),
                        Some(_) | None => Err(QueryBuilderError::ObjectTypeNotFound(
                            field_definition.r#type.name(),
                        )),
                    }?;

                    selection_set_field(
                        &alias.to_string(),
                        field_name,
                        field_arguments(
                            arguments,
                            map_query_arg,
                            field_definition,
                            parameters,
                            field_name,
                            &object_name,
                            variables,
                        )?,
                        fields,
                        field_definition,
                        parameters,
                        configuration,
                        variables,
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
        alias: if alias == field_name.inner() {
            None
        } else {
            Some(alias.to_string())
        },
        name: field_name.to_string(),
        arguments,
        directives: vec![],
        selection_set,
    }))
}

fn field_arguments<'a, A, M>(
    arguments: &BTreeMap<ArgumentName, A>,
    map_argument: M,
    field_definition: &ObjectFieldDefinition,
    parameters: &mut OperationParameters,
    field_name: &FieldName,
    object_name: &TypeName,
    variables: &BTreeMap<VariableName, serde_json::Value>,
) -> Result<Vec<(String, Value<'a, String>)>, QueryBuilderError>
where
    M: Fn(
        &A,
        &BTreeMap<VariableName, serde_json::Value>,
    ) -> Result<serde_json::Value, QueryBuilderError>,
{
    arguments
        .iter()
        .map(|(name, arg)| {
            let input_type = &field_definition
                .arguments
                .get(name)
                .ok_or(QueryBuilderError::ArgumentNotFound {
                    object: object_name.clone(),
                    field: field_name.clone(),
                    argument: name.clone(),
                })?
                .r#type;

            let value = map_argument(arg, variables)?;

            let value = parameters.insert(name, value, input_type);

            Ok((name.to_string(), value))
        })
        .collect()
}

fn map_query_arg(
    argument: &models::Argument,
    variables: &BTreeMap<VariableName, serde_json::Value>,
) -> Result<serde_json::Value, QueryBuilderError> {
    match argument {
        Argument::Variable { name } => variables
            .get(name)
            .map(std::borrow::ToOwned::to_owned)
            .ok_or_else(|| QueryBuilderError::MissingVariable(name.clone())),
        Argument::Literal { value } => Ok(value.to_owned()),
    }
}

fn map_arg(
    argument: &serde_json::Value,
    _variables: &BTreeMap<VariableName, serde_json::Value>,
) -> Result<serde_json::Value, QueryBuilderError> {
    Ok(argument.to_owned())
}

fn underlying_fields(nested_field: &NestedField) -> Option<&IndexMap<FieldName, models::Field>> {
    match nested_field {
        NestedField::Object(obj) => Some(&obj.fields),
        NestedField::Array(arr) => underlying_fields(&arr.fields),
        NestedField::Collection(_) => None,
    }
}

/// extract headers from the request_arguments object which is a string map.
fn extract_headers_from_request_arguments(
    request_arguments: Option<&BTreeMap<ArgumentName, serde_json::Value>>,
) -> Result<BTreeMap<String, String>, QueryBuilderError> {
    match request_arguments.and_then(|args| args.get("headers")) {
        None => Ok(BTreeMap::new()),
        Some(value) => serde_json::from_value::<Option<BTreeMap<String, String>>>(value.clone())
            .map(Option::unwrap_or_default)
            .map_err(|err| QueryBuilderError::Unexpected(err.to_string())),
    }
}
