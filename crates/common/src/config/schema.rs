use crate::config::{RequestConfig, ResponseConfig};
use graphql_parser::schema;
use ndc_models::{ArgumentName, FieldName, FunctionName, ProcedureName, ScalarTypeName, TypeName};
use std::{collections::BTreeMap, fmt::Display};

#[derive(Debug, Clone)]
pub struct SchemaDefinition {
    pub query_type_name: Option<TypeName>,
    pub query_fields: BTreeMap<FunctionName, ObjectFieldDefinition>,
    pub mutation_type_name: Option<TypeName>,
    pub mutation_fields: BTreeMap<ProcedureName, ObjectFieldDefinition>,
    pub definitions: BTreeMap<TypeName, TypeDef>,
}

impl SchemaDefinition {
    pub fn new(
        schema_document: &schema::Document<'_, String>,
        request_config: &RequestConfig,
        response_config: &ResponseConfig,
    ) -> Result<Self, SchemaDefinitionError> {
        let schema_definition = schema_document
            .definitions
            .iter()
            .find_map(|def| match def {
                schema::Definition::SchemaDefinition(schema) => Some(schema),
                schema::Definition::TypeDefinition(_)
                | schema::Definition::TypeExtension(_)
                | schema::Definition::DirectiveDefinition(_) => None,
            })
            .ok_or(SchemaDefinitionError::MissingSchemaType)?;

        // note: if there are duplicate definitions, the last one will stick.
        let definitions: BTreeMap<TypeName, TypeDef> = schema_document
            .definitions
            .iter()
            .filter_map(|definition| match definition {
                schema::Definition::SchemaDefinition(_) => None,
                schema::Definition::DirectiveDefinition(_) => None,
                schema::Definition::TypeExtension(_) => None,
                schema::Definition::TypeDefinition(type_definition) => match type_definition {
                    schema::TypeDefinition::Union(_) => None,
                    schema::TypeDefinition::Interface(_) => None,
                    schema::TypeDefinition::Scalar(scalar) => Some(TypeDef::new_scalar(scalar)),
                    schema::TypeDefinition::Object(object) => {
                        // skip query, mutation, subscription types
                        if schema_definition
                            .query
                            .as_ref()
                            .is_some_and(|query_type| query_type == &object.name)
                            || schema_definition
                                .subscription
                                .as_ref()
                                .is_some_and(|subscription_type| subscription_type == &object.name)
                            || schema_definition
                                .mutation
                                .as_ref()
                                .is_some_and(|mutation_type| mutation_type == &object.name)
                        {
                            None
                        } else {
                            Some(TypeDef::new_object(object))
                        }
                    }
                    schema::TypeDefinition::Enum(enum_definition) => {
                        Some(TypeDef::new_enum(enum_definition))
                    }
                    schema::TypeDefinition::InputObject(input_object) => {
                        Some(TypeDef::new_input_object(input_object))
                    }
                },
            })
            .collect();

        if definitions.contains_key(request_config.headers_type_name.inner()) {
            return Err(SchemaDefinitionError::HeaderTypeNameConflict(
                request_config.headers_type_name.to_owned(),
            ));
        }

        let query_type = schema_document
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
            });
        let mut query_fields = BTreeMap::new();

        if let Some(query_type) = query_type {
            for field in &query_type.fields {
                let query_field = field.name.to_owned().into();
                let response_type = response_config.query_response_type_name(&query_field);

                if definitions.contains_key(&response_type) {
                    return Err(SchemaDefinitionError::QueryResponseTypeConflict {
                        query_field,
                        response_type,
                    });
                }

                let field_definition = ObjectFieldDefinition::new(field);

                if field_definition
                    .arguments
                    .contains_key(&request_config.headers_argument)
                {
                    return Err(SchemaDefinitionError::QueryHeaderArgumentConflict {
                        query_field,
                        headers_argument: request_config.headers_argument.to_owned(),
                    });
                }

                query_fields.insert(field.name.to_owned().into(), field_definition);
            }
        }

        let mutation_type =
            schema_document
                .definitions
                .iter()
                .find_map(|def| match def {
                    schema::Definition::TypeDefinition(schema::TypeDefinition::Object(
                        mutation_type,
                    )) if schema_definition.mutation.as_ref().is_some_and(
                        |mutation_type_name| mutation_type_name == &mutation_type.name,
                    ) =>
                    {
                        Some(mutation_type)
                    }
                    _ => None,
                });
        let mut mutation_fields = BTreeMap::new();

        if let Some(mutation_type) = mutation_type {
            for field in &mutation_type.fields {
                let mutation_field = field.name.to_owned().into();
                let response_type = response_config.mutation_response_type_name(&mutation_field);

                if definitions.contains_key(&response_type) {
                    return Err(SchemaDefinitionError::MutationResponseTypeConflict {
                        mutation_field,
                        response_type,
                    });
                }

                let field_definition = ObjectFieldDefinition::new(field);

                if field_definition
                    .arguments
                    .contains_key(&request_config.headers_argument)
                {
                    return Err(SchemaDefinitionError::MutationHeaderArgumentConflict {
                        mutation_field,
                        headers_argument: request_config.headers_argument.to_owned(),
                    });
                }

                mutation_fields.insert(field.name.to_owned().into(), field_definition);
            }
        }

        Ok(Self {
            query_fields,
            query_type_name: schema_definition.query.to_owned().map(Into::into),
            mutation_fields,
            mutation_type_name: schema_definition.mutation.to_owned().map(Into::into),
            definitions,
        })
    }
}

#[derive(Debug, Clone)]
pub enum TypeRef {
    Named(String),
    List(Box<TypeRef>),
    NonNull(Box<TypeRef>),
}

impl TypeRef {
    fn new(type_reference: &schema::Type<String>) -> Self {
        match type_reference {
            schema::Type::NamedType(name) => Self::Named(name.to_owned()),
            schema::Type::ListType(underlying) => Self::List(Box::new(Self::new(underlying))),
            schema::Type::NonNullType(underlying) => Self::NonNull(Box::new(Self::new(underlying))),
        }
    }
    pub fn name(&self) -> TypeName {
        match self {
            TypeRef::Named(n) => n.to_owned().into(),
            TypeRef::List(underlying) | TypeRef::NonNull(underlying) => underlying.name(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum TypeDef {
    Scalar {
        description: Option<String>,
    },
    Enum {
        values: Vec<EnumValueDefinition>,
        description: Option<String>,
    },
    Object {
        fields: BTreeMap<FieldName, ObjectFieldDefinition>,
        description: Option<String>,
    },
    InputObject {
        fields: BTreeMap<FieldName, InputObjectFieldDefinition>,
        description: Option<String>,
    },
}

impl TypeDef {
    fn new_scalar(scalar_definition: &schema::ScalarType<String>) -> (TypeName, Self) {
        (
            scalar_definition.name.to_owned().into(),
            Self::Scalar {
                description: scalar_definition.description.to_owned(),
            },
        )
    }
    fn new_enum(enum_definition: &schema::EnumType<String>) -> (TypeName, Self) {
        (
            enum_definition.name.to_owned().into(),
            Self::Enum {
                values: enum_definition
                    .values
                    .iter()
                    .map(|value| EnumValueDefinition::new(value))
                    .collect(),
                description: enum_definition.description.to_owned(),
            },
        )
    }
    fn new_object(object_definition: &schema::ObjectType<String>) -> (TypeName, Self) {
        (
            object_definition.name.to_owned().into(),
            Self::Object {
                fields: object_definition
                    .fields
                    .iter()
                    .map(|field| {
                        (
                            field.name.to_owned().into(),
                            ObjectFieldDefinition::new(field),
                        )
                    })
                    .collect(),
                description: object_definition.description.to_owned(),
            },
        )
    }
    fn new_input_object(
        input_object_definition: &schema::InputObjectType<String>,
    ) -> (TypeName, Self) {
        (
            input_object_definition.name.to_owned().into(),
            Self::InputObject {
                fields: input_object_definition
                    .fields
                    .iter()
                    .map(|field| {
                        (
                            field.name.to_owned().into(),
                            InputObjectFieldDefinition::new(field),
                        )
                    })
                    .collect(),
                description: input_object_definition.description.to_owned(),
            },
        )
    }
}

#[derive(Debug, Clone)]
pub struct EnumValueDefinition {
    pub name: String,
    pub description: Option<String>,
}

impl EnumValueDefinition {
    fn new(value: &schema::EnumValue<String>) -> Self {
        Self {
            name: value.name.to_owned(),
            description: value.description.to_owned(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ObjectFieldDefinition {
    pub r#type: TypeRef,
    pub arguments: BTreeMap<ArgumentName, ObjectFieldArgumentDefinition>,
    pub description: Option<String>,
}

impl ObjectFieldDefinition {
    fn new(field: &schema::Field<String>) -> Self {
        Self {
            r#type: TypeRef::new(&field.field_type),
            arguments: field
                .arguments
                .iter()
                .map(|argument| {
                    (
                        argument.name.to_owned().into(),
                        ObjectFieldArgumentDefinition::new(argument),
                    )
                })
                .collect(),
            description: field.description.to_owned(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ObjectFieldArgumentDefinition {
    pub r#type: TypeRef,
    pub description: Option<String>,
}

impl ObjectFieldArgumentDefinition {
    fn new(argument: &schema::InputValue<String>) -> Self {
        Self {
            r#type: TypeRef::new(&argument.value_type),
            description: argument.description.to_owned(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct InputObjectFieldDefinition {
    pub r#type: TypeRef,
    pub description: Option<String>,
}

impl InputObjectFieldDefinition {
    fn new(field: &schema::InputValue<String>) -> Self {
        Self {
            r#type: TypeRef::new(&field.value_type),
            description: field.description.to_owned(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum SchemaDefinitionError {
    MissingSchemaType,
    HeaderTypeNameConflict(ScalarTypeName),
    QueryHeaderArgumentConflict {
        query_field: FunctionName,
        headers_argument: ArgumentName,
    },
    MutationHeaderArgumentConflict {
        mutation_field: ProcedureName,
        headers_argument: ArgumentName,
    },
    QueryResponseTypeConflict {
        query_field: FunctionName,
        response_type: TypeName,
    },
    MutationResponseTypeConflict {
        mutation_field: ProcedureName,
        response_type: TypeName,
    },
}

impl std::error::Error for SchemaDefinitionError {}

impl Display for SchemaDefinitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SchemaDefinitionError::MissingSchemaType => write!(
                f,
                "Missing Schema Type: expected schema file with schema definition"
            ),
            SchemaDefinitionError::HeaderTypeNameConflict(name) => write!(f, "HeaderMap name conflict: Another type with name {name} exists. Change the name under request.headerTypeName"),
            SchemaDefinitionError::QueryHeaderArgumentConflict {
                query_field,
                headers_argument,
            } => write!(f, "Query Headers argument conflict: Query field {query_field} has an argument with name {headers_argument}. Change the headers argument name under request.headerArgument"),
            SchemaDefinitionError::MutationHeaderArgumentConflict {
                mutation_field,
                headers_argument,
            } => write!(f, "Mutation Headers argument conflict: Mutation field {mutation_field} has an argument with name {headers_argument}. Change the headers argument name under request.headerArgument"),
            SchemaDefinitionError::QueryResponseTypeConflict {
                query_field,
                response_type,
            } => write!(f, "ResponseType name conflict for Query field {query_field}: A type with name {response_type} already exist. Change the response typename prefix or suffix under  response.typeNamePrefix or response.typeNameSuffix"),
            SchemaDefinitionError::MutationResponseTypeConflict {
                mutation_field,
                response_type,
            } => write!(f, "ResponseType name conflict for Mutation field {mutation_field}: A type with name {response_type} already exist. Change the response typename prefix or suffix under  response.typeNamePrefix or response.typeNameSuffix"),
        }
    }
}
