use crate::config::{
    schema::{
        InputObjectFieldDefinition, ObjectFieldArgumentDefinition, ObjectFieldDefinition,
        SchemaDefinition, TypeDef, TypeRef,
    },
    RequestConfig, ResponseConfig,
};
use ndc_models::{
    self as models, ArgumentInfo, ArgumentName, FieldName, SchemaResponse, Type, TypeName,
    TypeRepresentation,
};
use std::{collections::BTreeMap, iter};

pub fn schema_response(
    schema: &SchemaDefinition,
    request: &RequestConfig,
    response: &ResponseConfig,
) -> SchemaResponse {
    let forward_request_headers = !request.forward_headers.is_empty();
    let forward_response_headers = !response.forward_headers.is_empty();
    let has_polymorphic_types = schema.definitions.values().any(|definition| {
        matches!(
            definition,
            TypeDef::Interface { .. } | TypeDef::Union { .. }
        )
    });

    let mut scalar_types: BTreeMap<_, _> = schema
        .definitions
        .iter()
        .filter_map(|(name, typedef)| match typedef {
            TypeDef::Object { .. }
            | TypeDef::InputObject { .. }
            | TypeDef::Interface { .. }
            | TypeDef::Union { .. } => None,
            TypeDef::Scalar { description: _ } => Some((
                name.to_owned().into(),
                models::ScalarType {
                    representation: type_name_to_representation(name),
                    aggregate_functions: BTreeMap::new(),
                    comparison_operators: BTreeMap::new(),
                    extraction_functions: BTreeMap::new(),
                },
            )),
            TypeDef::Enum {
                values,
                description: _,
            } => Some((
                name.to_owned().into(),
                models::ScalarType {
                    representation: models::TypeRepresentation::Enum {
                        one_of: values.iter().map(|value| value.name.to_owned()).collect(),
                    },
                    aggregate_functions: BTreeMap::new(),
                    comparison_operators: BTreeMap::new(),
                    extraction_functions: BTreeMap::new(),
                },
            )),
        })
        .collect();

    if has_polymorphic_types && !scalar_types.contains_key("String") {
        scalar_types.insert(
            "String".into(),
            models::ScalarType {
                representation: models::TypeRepresentation::String,
                aggregate_functions: BTreeMap::new(),
                comparison_operators: BTreeMap::new(),
                extraction_functions: BTreeMap::new(),
            },
        );
    }

    // add headers type name for either header forwarding or request-level arguments
    scalar_types.insert(
        request.headers_type_name.to_owned(),
        models::ScalarType {
            representation: models::TypeRepresentation::JSON,
            aggregate_functions: BTreeMap::new(),
            comparison_operators: BTreeMap::new(),
            extraction_functions: BTreeMap::new(),
        },
    );

    let mut object_types: BTreeMap<_, _> = BTreeMap::new();
    for (name, typedef) in &schema.definitions {
        match typedef {
            TypeDef::Scalar { .. } | TypeDef::Enum { .. } => {}
            TypeDef::Object {
                fields,
                description,
            } => {
                object_types.insert(
                    name.to_owned().into(),
                    models::ObjectType {
                        description: description.to_owned(),
                        fields: fields.iter().map(map_object_field).collect(),
                        foreign_keys: BTreeMap::new(),
                    },
                );
            }
            TypeDef::Interface {
                fields,
                possible_types,
                description,
            } => {
                let mut variant_types = Vec::new();
                for concrete_type in possible_types {
                    let Some(TypeDef::Object {
                        fields: concrete_fields,
                        ..
                    }) = schema.definitions.get(concrete_type)
                    else {
                        continue;
                    };

                    let exclusive_fields =
                        interface_exclusive_fields(fields, concrete_fields);
                    if exclusive_fields.is_empty() {
                        continue;
                    }

                    let variant_type_name = polymorphic_variant_type_name(name, concrete_type);
                    object_types.insert(
                        variant_type_name.to_owned().into(),
                        models::ObjectType {
                            description: Some(format!(
                                "Fields available only when runtime type is {concrete_type} for polymorphic type {name}"
                            )),
                            fields: exclusive_fields.iter().map(map_object_field).collect(),
                            foreign_keys: BTreeMap::new(),
                        },
                    );
                    variant_types
                        .push((concrete_type.to_string(), variant_type_name.to_string()));
                }

                object_types.insert(
                    name.to_owned().into(),
                    polymorphic_object_type(name, description, Some(fields), variant_types.into_iter()),
                );
            }
            TypeDef::Union {
                members,
                description,
            } => {
                object_types.insert(
                    name.to_owned().into(),
                    polymorphic_object_type(
                        name,
                        description,
                        None,
                        members.iter().map(|member| (member.to_string(), member.to_string())),
                    ),
                );
            }
            TypeDef::InputObject {
                fields,
                description,
            } => {
                object_types.insert(
                    name.to_owned().into(),
                    models::ObjectType {
                        description: description.to_owned(),
                        fields: fields.iter().map(map_input_object_field).collect(),
                        foreign_keys: BTreeMap::new(),
                    },
                );
            }
        }
    }

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
                foreign_keys: BTreeMap::new(),
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

    // request-level arguments
    let common_request_arguments: BTreeMap<ArgumentName, ArgumentInfo> = BTreeMap::from([(
        "headers".into(),
        ArgumentInfo {
            description: Some(
                "Headers to be merged into original request headers of graphql requests"
                    .to_string(),
            ),
            argument_type: Type::Nullable {
                underlying_type: Box::new(Type::Named {
                    name: TypeName::from(request.headers_type_name.as_str()),
                }),
            },
        },
    )]);

    let request_level_arguments = ndc_models::RequestLevelArguments {
        query_arguments: common_request_arguments.clone(),
        mutation_arguments: common_request_arguments,
        relational_query_arguments: BTreeMap::new(),
    };

    models::SchemaResponse {
        scalar_types,
        object_types,
        collections: vec![],
        functions,
        procedures,
        capabilities: None,
        request_arguments: Some(request_level_arguments),
    }
}

fn polymorphic_object_type<I>(
    type_name: &TypeName,
    description: &Option<String>,
    common_fields: Option<&BTreeMap<FieldName, ObjectFieldDefinition>>,
    concrete_types: I,
) -> models::ObjectType
where
    I: Iterator<Item = (String, String)>,
{
    let mut fields: BTreeMap<FieldName, models::ObjectField> = common_fields
        .map(|fields| fields.iter().map(map_object_field).collect())
        .unwrap_or_default();

    fields.insert(
        "__typename".into(),
        models::ObjectField {
            description: Some(format!(
                "Concrete GraphQL type name for polymorphic type {type_name}"
            )),
            r#type: models::Type::Named {
                name: "String".to_owned().into(),
            },
            arguments: BTreeMap::new(),
        },
    );

    for (concrete_type, concrete_type_output) in concrete_types {
        fields.insert(
            format!("on_{concrete_type}").into(),
            models::ObjectField {
                description: Some(format!(
                    "Fields available when runtime type is {concrete_type}"
                )),
                r#type: models::Type::Nullable {
                    underlying_type: Box::new(models::Type::Named {
                        name: concrete_type_output.to_owned().into(),
                    }),
                },
                arguments: BTreeMap::new(),
            },
        );
    }

    models::ObjectType {
        description: description.to_owned(),
        fields,
        foreign_keys: BTreeMap::new(),
    }
}

fn interface_exclusive_fields(
    common_fields: &BTreeMap<FieldName, ObjectFieldDefinition>,
    concrete_fields: &BTreeMap<FieldName, ObjectFieldDefinition>,
) -> BTreeMap<FieldName, ObjectFieldDefinition> {
    concrete_fields
        .iter()
        .filter(|(field_name, _)| !common_fields.contains_key(*field_name))
        .map(|(field_name, field_definition)| {
            (field_name.to_owned(), field_definition.to_owned())
        })
        .collect()
}

fn polymorphic_variant_type_name(type_name: &TypeName, concrete_type: &TypeName) -> TypeName {
    format!("{type_name}On{concrete_type}").into()
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

// Guess the type representation by common GraphQL scalar names such as String, Int, Float, Boolean, etc...
// https://spec.graphql.org/draft/#sec-Scalars.Built-in-Scalars
fn type_name_to_representation(name: &TypeName) -> TypeRepresentation {
    match name.to_string().to_lowercase().as_str() {
        "bool" | "boolean" => TypeRepresentation::Boolean,
        "int8" | "tinyint" => TypeRepresentation::Int8,
        "int16" | "smallint" => TypeRepresentation::Int16,
        "int" | "int32" | "serial" => TypeRepresentation::Int32,
        "int64" => TypeRepresentation::Int64,
        "bigint" | "bigserial" => TypeRepresentation::BigInteger,
        "float8" | "float16" | "float32" => TypeRepresentation::Float32,
        "float" | "float64" => TypeRepresentation::Float64,
        "date" => TypeRepresentation::Date,
        "timestamptz" | "datetime" => TypeRepresentation::TimestampTZ,
        "id" | "string" | "uuid" | "text" | "citext" | "varchar" => TypeRepresentation::String,
        _ => TypeRepresentation::JSON,
    }
}
