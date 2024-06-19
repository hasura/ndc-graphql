use common::{
    config::ServerConfig,
    schema::{
        InputObjectFieldDefinition, ObjectFieldArgumentDefinition, ObjectFieldDefinition, TypeRef,
    },
};
use ndc_sdk::models;
use std::{collections::BTreeMap, iter};

pub fn schema_response(configuration: &ServerConfig) -> models::SchemaResponse {
    let mut scalar_types: BTreeMap<_, _> = configuration
        .schema
        .definitions
        .iter()
        .filter_map(|(name, typedef)| match typedef {
            common::schema::TypeDef::Object { .. }
            | common::schema::TypeDef::InputObject { .. } => None,
            common::schema::TypeDef::Scalar { description: _ } => Some((
                name.to_owned(),
                models::ScalarType {
                    representation: None,
                    aggregate_functions: BTreeMap::new(),
                    comparison_operators: BTreeMap::new(),
                },
            )),
            common::schema::TypeDef::Enum {
                values,
                description: _,
            } => Some((
                name.to_owned(),
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

    scalar_types.insert(
        configuration.request.headers_type_name.to_owned(),
        models::ScalarType {
            representation: Some(models::TypeRepresentation::JSON),
            aggregate_functions: BTreeMap::new(),
            comparison_operators: BTreeMap::new(),
        },
    );

    let mut object_types: BTreeMap<_, _> = configuration
        .schema
        .definitions
        .iter()
        .filter_map(|(name, typedef)| match typedef {
            common::schema::TypeDef::Scalar { .. } | common::schema::TypeDef::Enum { .. } => None,
            common::schema::TypeDef::Object {
                fields,
                description,
            } => Some((
                name.to_owned(),
                models::ObjectType {
                    description: description.to_owned(),
                    fields: fields.iter().map(map_object_field).collect(),
                },
            )),
            common::schema::TypeDef::InputObject {
                fields,
                description,
            } => Some((
                name.to_owned(),
                models::ObjectType {
                    description: description.to_owned(),
                    fields: fields.iter().map(map_input_object_field).collect(),
                },
            )),
        })
        .collect();

    let response_type =
        |field: &ObjectFieldDefinition, operation_type: &str, operation_name: &str| {
            models::ObjectType {
                description: Some(format!(
                    "Response type for {operation_type} {operation_name}"
                )),
                fields: BTreeMap::from_iter(vec![
                    (
                        configuration.response.headers_field.to_owned(),
                        models::ObjectField {
                            description: None,
                            r#type: models::Type::Named {
                                name: configuration.request.headers_type_name.to_owned(),
                            },
                            arguments: BTreeMap::new(),
                        },
                    ),
                    (
                        configuration.response.response_field.to_owned(),
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

    for (name, field) in &configuration.schema.query_fields {
        let response_type_name = configuration.response.query_response_type_name(name);

        object_types.insert(
            response_type_name.clone(),
            response_type(field, "function", name),
        );

        functions.push(models::FunctionInfo {
            name: name.to_owned(),
            description: field.description.to_owned(),
            arguments: field
                .arguments
                .iter()
                .map(map_argument)
                .chain(iter::once((
                    configuration.request.headers_argument.to_owned(),
                    models::ArgumentInfo {
                        description: None,
                        argument_type: models::Type::Named {
                            name: configuration.request.headers_type_name.to_owned(),
                        },
                    },
                )))
                .collect(),
            result_type: models::Type::Named {
                name: response_type_name,
            },
        });
    }

    let mut procedures = vec![];

    for (name, field) in &configuration.schema.mutation_fields {
        let response_type_name = configuration.response.mutation_response_type_name(name);

        object_types.insert(
            response_type_name.clone(),
            response_type(field, "procedure", name),
        );

        procedures.push(models::ProcedureInfo {
            name: name.to_owned(),
            description: field.description.to_owned(),
            arguments: field
                .arguments
                .iter()
                .map(map_argument)
                .chain(iter::once((
                    configuration.request.headers_argument.to_owned(),
                    models::ArgumentInfo {
                        description: None,
                        argument_type: models::Type::Named {
                            name: configuration.request.headers_type_name.to_owned(),
                        },
                    },
                )))
                .collect(),
            result_type: models::Type::Named {
                name: response_type_name,
            },
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
    (name, field): (&String, &ObjectFieldDefinition),
) -> (String, models::ObjectField) {
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
    (name, argument): (&String, &ObjectFieldArgumentDefinition),
) -> (String, models::ArgumentInfo) {
    (
        name.to_owned(),
        models::ArgumentInfo {
            description: argument.description.to_owned(),
            argument_type: typeref_to_ndc_type(&argument.r#type),
        },
    )
}

fn map_input_object_field(
    (name, field): (&String, &InputObjectFieldDefinition),
) -> (String, models::ObjectField) {
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
            underlying_type: Box::new(models::Type::Named { name: name.into() }),
        },
        TypeRef::List(inner) => models::Type::Nullable {
            underlying_type: Box::new(models::Type::Array {
                element_type: Box::new(typeref_to_ndc_type(inner)),
            }),
        },
        TypeRef::NonNull(inner) => match &**inner {
            TypeRef::Named(name) => models::Type::Named { name: name.into() },
            TypeRef::List(inner) => models::Type::Array {
                element_type: Box::new(typeref_to_ndc_type(inner)),
            },
            // ignore (illegal) double non-null assertions. This shouln't happen anyways
            TypeRef::NonNull(_) => typeref_to_ndc_type(inner),
        },
    }
}
