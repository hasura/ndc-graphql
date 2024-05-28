use graphql_parser::schema;
use ndc_sdk::models;
use std::collections::BTreeMap;

pub fn schema_from_graphql_document(
    schema_document: &schema::Document<'_, String>,
) -> models::SchemaResponse {
    let schema_definition = schema_document
        .definitions
        .iter()
        .find_map(|def| match def {
            schema::Definition::SchemaDefinition(schema) => Some(schema),
            schema::Definition::TypeDefinition(_)
            | schema::Definition::TypeExtension(_)
            | schema::Definition::DirectiveDefinition(_) => None,
        })
        .expect("schema type");

    let scalar_types = schema_document
        .definitions
        .iter()
        .filter_map(|def| {
            // todo: enums
            if let schema::Definition::TypeDefinition(schema::TypeDefinition::Scalar(s)) = def {
                let type_name = s.name.to_owned();

                let type_definition = models::ScalarType {
                    representation: None, // todo: figure out type representation for scalar types
                    aggregate_functions: BTreeMap::new(),
                    comparison_operators: BTreeMap::new(),
                };

                Some((type_name, type_definition))
            } else if let schema::Definition::TypeDefinition(schema::TypeDefinition::Enum(e)) = def
            {
                let type_name = e.name.to_owned();

                let type_definition = models::ScalarType {
                    representation: None, // todo: figure out type representation for enum types
                    aggregate_functions: BTreeMap::new(),
                    comparison_operators: BTreeMap::new(),
                };

                Some((type_name, type_definition))
            } else {
                None
            }
        })
        .collect();

    let object_types = schema_document
        .definitions
        .iter()
        .filter_map(|def| {
            if let schema::Definition::TypeDefinition(schema::TypeDefinition::Object(object_type)) =
                def
            {
                let name = object_type.name.to_owned();

                // skip query, mutation, subscription types
                if schema_definition
                    .query
                    .as_ref()
                    .is_some_and(|query_type| query_type == &name)
                    || schema_definition
                        .subscription
                        .as_ref()
                        .is_some_and(|subscription_type| subscription_type == &name)
                    || schema_definition
                        .mutation
                        .as_ref()
                        .is_some_and(|mutation_type| mutation_type == &name)
                {
                    return None;
                }

                let fields = object_type
                    .fields
                    .iter()
                    .map(|field| {
                        (
                            field.name.to_owned(),
                            models::ObjectField {
                                description: field.description.to_owned(),
                                r#type: to_ndc_type(&field.field_type),
                            },
                        )
                    })
                    .collect();

                Some((
                    name,
                    models::ObjectType {
                        description: object_type.description.to_owned(),
                        fields,
                    },
                ))
            } else if let schema::Definition::TypeDefinition(schema::TypeDefinition::InputObject(
                input_type,
            )) = def
            {
                let name = input_type.name.to_owned();
                let fields = input_type
                    .fields
                    .iter()
                    .map(|field| {
                        (
                            field.name.to_owned(),
                            models::ObjectField {
                                description: field.description.to_owned(),
                                r#type: to_ndc_type(&field.value_type),
                            },
                        )
                    })
                    .collect();

                Some((
                    name,
                    models::ObjectType {
                        description: input_type.description.to_owned(),
                        fields,
                    },
                ))
            } else {
                None
            }
        })
        .collect();

    let functions = schema_document
        .definitions
        .iter()
        .find_map(|def| match def {
            schema::Definition::TypeDefinition(schema::TypeDefinition::Object(query_type))
                if schema_definition
                    .query
                    .as_ref()
                    .is_some_and(|query_type_name| query_type_name == &query_type.name) =>
            {
                Some(query_type)
            }
            _ => None,
        })
        .map(|query_type| {
            query_type
                .fields
                .iter()
                .map(|field| models::FunctionInfo {
                    name: field.name.to_owned(),
                    description: field.description.to_owned(),
                    arguments: field
                        .arguments
                        .iter()
                        .map(|arg| {
                            (
                                arg.name.to_owned(),
                                models::ArgumentInfo {
                                    description: arg.description.to_owned(),
                                    argument_type: to_ndc_type(&arg.value_type),
                                },
                            )
                        })
                        .collect(),
                    result_type: to_ndc_type(&field.field_type),
                })
                .collect()
        })
        .unwrap_or_default();

    let procedures = schema_document
        .definitions
        .iter()
        .find_map(|def| match def {
            schema::Definition::TypeDefinition(schema::TypeDefinition::Object(mutation_type))
                if schema_definition
                    .mutation
                    .as_ref()
                    .is_some_and(|mutation_type_name| {
                        mutation_type_name == &mutation_type.name
                    }) =>
            {
                Some(mutation_type)
            }
            _ => None,
        })
        .map(|mutation_type| {
            mutation_type
                .fields
                .iter()
                .map(|field| models::ProcedureInfo {
                    name: field.name.to_owned(),
                    description: field.description.to_owned(),
                    arguments: field
                        .arguments
                        .iter()
                        .map(|arg| {
                            (
                                arg.name.to_owned(),
                                models::ArgumentInfo {
                                    description: arg.description.to_owned(),
                                    argument_type: to_ndc_type(&arg.value_type),
                                },
                            )
                        })
                        .collect(),
                    result_type: to_ndc_type(&field.field_type),
                })
                .collect()
        })
        .unwrap_or_default();

    models::SchemaResponse {
        scalar_types,
        object_types,
        collections: vec![],
        functions,
        procedures,
    }
}

fn to_ndc_type(field_type: &schema::Type<String>) -> models::Type {
    match field_type {
        schema::Type::NamedType(name) => models::Type::Nullable {
            underlying_type: Box::new(models::Type::Named { name: name.into() }),
        },
        schema::Type::ListType(inner) => models::Type::Nullable {
            underlying_type: Box::new(models::Type::Array {
                element_type: Box::new(to_ndc_type(inner)),
            }),
        },
        schema::Type::NonNullType(inner) => match &**inner {
            schema::Type::NamedType(name) => models::Type::Named { name: name.into() },
            schema::Type::ListType(inner) => models::Type::Array {
                element_type: Box::new(to_ndc_type(inner)),
            },
            schema::Type::NonNullType(_) => {
                todo!("Nested non-null (T!!) is not valid graphql. Todo: handle as error")
            }
        },
    }
}
