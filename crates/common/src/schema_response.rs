use crate::config::{
    schema::{
        InputObjectFieldDefinition, ObjectFieldArgumentDefinition, ObjectFieldDefinition,
        SchemaDefinition, TypeDef, TypeRef,
    },
    RequestConfig, ResponseConfig,
};
use ndc_models::{self as models, ArgumentName, FieldName, SchemaResponse};
use std::{collections::BTreeMap, iter};

pub fn schema_response(
    schema: &SchemaDefinition,
    request: &RequestConfig,
    response: &ResponseConfig,
) -> SchemaResponse {
    let forward_request_headers = !request.forward_headers.is_empty();
    let forward_response_headers = !response.forward_headers.is_empty();

    let mut scalar_types: BTreeMap<_, _> = schema
        .definitions
        .iter()
        .filter_map(|(name, typedef)| match typedef {
            TypeDef::Object { .. } | TypeDef::InputObject { .. } => None,
            TypeDef::Scalar { description: _ } => Some((
                name.to_owned().into(),
                models::ScalarType {
                    representation: None,
                    aggregate_functions: BTreeMap::new(),
                    comparison_operators: BTreeMap::new(),
                },
            )),
            TypeDef::Enum {
                values,
                description: _,
            } => Some((
                name.to_owned().into(),
                models::ScalarType {
                    representation: Some(models::TypeRepresentation::Enum {
                        one_of: values.iter().map(|value| value.name.to_owned()).collect(),
                    }),
                    aggregate_functions: BTreeMap::new(),
                    comparison_operators: BTreeMap::new(),
                },
            )),
        })
        .collect();

    if forward_request_headers {
        scalar_types.insert(
            request.headers_type_name.to_owned(),
            models::ScalarType {
                representation: Some(models::TypeRepresentation::JSON),
                aggregate_functions: BTreeMap::new(),
                comparison_operators: BTreeMap::new(),
            },
        );
    }

    let mut object_types: BTreeMap<_, _> = schema
        .definitions
        .iter()
        .filter_map(|(name, typedef)| match typedef {
            TypeDef::Scalar { .. } | TypeDef::Enum { .. } => None,
            TypeDef::Object {
                fields,
                description,
            } => Some((
                name.to_owned().into(),
                models::ObjectType {
                    description: description.to_owned(),
                    fields: fields.iter().map(map_object_field).collect(),
                },
            )),
            TypeDef::InputObject {
                fields,
                description,
            } => Some((
                name.to_owned().into(),
                models::ObjectType {
                    description: description.to_owned(),
                    fields: fields.iter().map(map_input_object_field).collect(),
                },
            )),
        })
        .collect();

    let response_type =
        |field: &ObjectFieldDefinition, operation_type: &str, operation_name: &FieldName| {
            models::ObjectType {
                description: Some(format!(
                    "Response type for {operation_type} {operation_name}"
                )),
                fields: BTreeMap::from_iter(vec![
                    (
                        response.headers_field.to_owned(),
                        models::ObjectField {
                            description: None,
                            r#type: models::Type::Named {
                                name: request.headers_type_name.inner().to_owned(),
                            },
                            arguments: BTreeMap::new(),
                        },
                    ),
                    (
                        response.response_field.to_owned(),
                        models::ObjectField {
                            description: None,
                            r#type: typeref_to_ndc_type(&field.r#type),
                            arguments: BTreeMap::new(),
                        },
                    ),
                ]),
            }
        };

    let mut functions = vec![];

    for (name, field) in &schema.query_fields {
        let arguments = field.arguments.iter().map(map_argument);
        let arguments = if forward_request_headers {
            arguments
                .chain(iter::once((
                    request.headers_argument.to_owned(),
                    models::ArgumentInfo {
                        description: None,
                        argument_type: models::Type::Named {
                            name: request.headers_type_name.inner().to_owned(),
                        },
                    },
                )))
                .collect()
        } else {
            arguments.collect()
        };

        let result_type = if forward_response_headers {
            let response_type_name = response.query_response_type_name(name);

            object_types.insert(
                response_type_name.to_owned().into(),
                response_type(field, "function", &name.to_string().into()),
            );

            models::Type::Named {
                name: response_type_name,
            }
        } else {
            typeref_to_ndc_type(&field.r#type)
        };

        functions.push(models::FunctionInfo {
            name: name.to_string().into(),
            description: field.description.to_owned(),
            arguments,
            result_type,
        });
    }

    let mut procedures = vec![];

    for (name, field) in &schema.mutation_fields {
        let arguments = field.arguments.iter().map(map_argument);
        let arguments = if forward_request_headers {
            arguments
                .chain(iter::once((
                    request.headers_argument.to_owned(),
                    models::ArgumentInfo {
                        description: None,
                        argument_type: models::Type::Named {
                            name: request.headers_type_name.inner().to_owned(),
                        },
                    },
                )))
                .collect()
        } else {
            arguments.collect()
        };

        let result_type = if forward_response_headers {
            let response_type_name = response.mutation_response_type_name(name);

            object_types.insert(
                response_type_name.to_owned().into(),
                response_type(field, "procedure", &name.to_string().into()),
            );

            models::Type::Named {
                name: response_type_name,
            }
        } else {
            typeref_to_ndc_type(&field.r#type)
        };

        procedures.push(models::ProcedureInfo {
            name: name.to_string().into(),
            description: field.description.to_owned(),
            arguments,
            result_type,
        });
    }

    models::SchemaResponse {
        scalar_types,
        object_types,
        collections: vec![],
        functions,
        procedures,
    }
}

fn map_object_field(
    (name, field): (&FieldName, &ObjectFieldDefinition),
) -> (FieldName, models::ObjectField) {
    (
        name.to_owned(),
        models::ObjectField {
            description: field.description.to_owned(),
            r#type: typeref_to_ndc_type(&field.r#type),
            arguments: field.arguments.iter().map(map_argument).collect(),
        },
    )
}

fn map_argument(
    (name, argument): (&ArgumentName, &ObjectFieldArgumentDefinition),
) -> (ArgumentName, models::ArgumentInfo) {
    (
        name.to_owned(),
        models::ArgumentInfo {
            description: argument.description.to_owned(),
            argument_type: typeref_to_ndc_type(&argument.r#type),
        },
    )
}

fn map_input_object_field(
    (name, field): (&FieldName, &InputObjectFieldDefinition),
) -> (FieldName, models::ObjectField) {
    (
        name.to_owned(),
        models::ObjectField {
            description: field.description.to_owned(),
            r#type: typeref_to_ndc_type(&field.r#type),
            arguments: BTreeMap::new(),
        },
    )
}

fn typeref_to_ndc_type(typeref: &TypeRef) -> models::Type {
    match typeref {
        TypeRef::Named(name) => models::Type::Nullable {
            underlying_type: Box::new(models::Type::Named {
                name: name.to_owned().into(),
            }),
        },
        TypeRef::List(inner) => models::Type::Nullable {
            underlying_type: Box::new(models::Type::Array {
                element_type: Box::new(typeref_to_ndc_type(inner)),
            }),
        },
        TypeRef::NonNull(inner) => match &**inner {
            TypeRef::Named(name) => models::Type::Named {
                name: name.to_owned().into(),
            },
            TypeRef::List(inner) => models::Type::Array {
                element_type: Box::new(typeref_to_ndc_type(inner)),
            },
            // ignore (illegal) double non-null assertions. This shouln't happen anyways
            TypeRef::NonNull(_) => typeref_to_ndc_type(inner),
        },
    }
}
