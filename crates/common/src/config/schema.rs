use crate::config::{RequestConfig, ResponseConfig};
use graphql_parser::schema;
use ndc_models::{ArgumentName, FieldName, FunctionName, ProcedureName, ScalarTypeName, TypeName};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Display,
};

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
        let mut definitions: BTreeMap<TypeName, TypeDef> = BTreeMap::new();
        let mut object_implements: Vec<(TypeName, Vec<TypeName>)> = vec![];

        for definition in &schema_document.definitions {
            let (type_name, type_def) = match definition {
                schema::Definition::SchemaDefinition(_)
                | schema::Definition::DirectiveDefinition(_)
                | schema::Definition::TypeExtension(_) => continue,
                schema::Definition::TypeDefinition(type_definition) => match type_definition {
                    schema::TypeDefinition::Scalar(scalar) => TypeDef::new_scalar(scalar),
                    schema::TypeDefinition::Object(object) => {
                        // skip query, mutation, subscription root types
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
                            continue;
                        }

                        let (name, def, implements_interfaces) = TypeDef::new_object(object);
                        object_implements.push((name.clone(), implements_interfaces));
                        (name, def)
                    }
                    schema::TypeDefinition::Enum(enum_definition) => {
                        TypeDef::new_enum(enum_definition)
                    }
                    schema::TypeDefinition::InputObject(input_object) => {
                        TypeDef::new_input_object(input_object)
                    }
                    schema::TypeDefinition::Interface(interface) => {
                        TypeDef::new_interface(interface)
                    }
                    schema::TypeDefinition::Union(union) => TypeDef::new_union(union),
                },
            };

            definitions.insert(type_name, type_def);
        }

        for (object_name, interfaces) in object_implements {
            for interface_name in interfaces {
                if let Some(TypeDef::Interface { possible_types, .. }) =
                    definitions.get_mut(&interface_name)
                {
                    possible_types.insert(object_name.clone());
                }
            }
        }

        if definitions.contains_key(request_config.headers_type_name.inner()) {
            return Err(SchemaDefinitionError::HeaderTypeNameConflict(
                request_config.headers_type_name.to_owned(),
            ));
        }

        let root_type_names: BTreeSet<TypeName> = vec![
            schema_definition
                .query
                .as_ref()
                .map(|name| name.to_owned().into()),
            schema_definition
                .mutation
                .as_ref()
                .map(|name| name.to_owned().into()),
            schema_definition
                .subscription
                .as_ref()
                .map(|name| name.to_owned().into()),
        ]
        .into_iter()
        .flatten()
        .collect();

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

        validate_definition_type_references(
            &definitions,
            &query_fields,
            &mutation_fields,
            &root_type_names,
        )?;

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
    Interface {
        fields: BTreeMap<FieldName, ObjectFieldDefinition>,
        possible_types: BTreeSet<TypeName>,
        description: Option<String>,
    },
    Union {
        members: BTreeSet<TypeName>,
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
    fn new_object(
        object_definition: &schema::ObjectType<String>,
    ) -> (TypeName, Self, Vec<TypeName>) {
        let interfaces = object_definition
            .implements_interfaces
            .iter()
            .map(|interface_name| interface_name.to_owned().into())
            .collect();

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
            interfaces,
        )
    }
    fn new_interface(interface_definition: &schema::InterfaceType<String>) -> (TypeName, Self) {
        (
            interface_definition.name.to_owned().into(),
            Self::Interface {
                fields: interface_definition
                    .fields
                    .iter()
                    .map(|field| {
                        (
                            field.name.to_owned().into(),
                            ObjectFieldDefinition::new(field),
                        )
                    })
                    .collect(),
                possible_types: BTreeSet::new(),
                description: interface_definition.description.to_owned(),
            },
        )
    }
    fn new_union(union_definition: &schema::UnionType<String>) -> (TypeName, Self) {
        (
            union_definition.name.to_owned().into(),
            Self::Union {
                members: union_definition
                    .types
                    .iter()
                    .map(|type_name| type_name.to_owned().into())
                    .collect(),
                description: union_definition.description.to_owned(),
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
    UnknownTypeReference {
        referenced_type: TypeName,
        location: String,
    },
    InterfaceWithoutConcreteTypes(TypeName),
    InterfaceConcreteTypeNotObject {
        interface_name: TypeName,
        concrete_type: TypeName,
    },
    InterfaceConcreteTypeNotFound {
        interface_name: TypeName,
        concrete_type: TypeName,
    },
    UnionWithoutConcreteTypes(TypeName),
    UnionMemberNotObject {
        union_name: TypeName,
        member_type: TypeName,
    },
    UnionMemberTypeNotFound {
        union_name: TypeName,
        member_type: TypeName,
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
            SchemaDefinitionError::UnknownTypeReference {
                referenced_type,
                location,
            } => write!(
                f,
                "Unknown type reference: {referenced_type} was referenced by {location}, but no matching type exists"
            ),
            SchemaDefinitionError::InterfaceWithoutConcreteTypes(interface_name) => write!(
                f,
                "Cannot lower interface {interface_name}: no implementing object types were found"
            ),
            SchemaDefinitionError::InterfaceConcreteTypeNotObject {
                interface_name,
                concrete_type,
            } => write!(
                f,
                "Cannot lower interface {interface_name}: concrete type {concrete_type} is not an object type"
            ),
            SchemaDefinitionError::InterfaceConcreteTypeNotFound {
                interface_name,
                concrete_type,
            } => write!(
                f,
                "Cannot lower interface {interface_name}: concrete type {concrete_type} was not found"
            ),
            SchemaDefinitionError::UnionWithoutConcreteTypes(union_name) => write!(
                f,
                "Cannot lower union {union_name}: no concrete object members were found"
            ),
            SchemaDefinitionError::UnionMemberNotObject {
                union_name,
                member_type,
            } => write!(
                f,
                "Cannot lower union {union_name}: member type {member_type} is not an object type"
            ),
            SchemaDefinitionError::UnionMemberTypeNotFound {
                union_name,
                member_type,
            } => write!(
                f,
                "Cannot lower union {union_name}: member type {member_type} was not found"
            ),
        }
    }
}

fn validate_definition_type_references(
    definitions: &BTreeMap<TypeName, TypeDef>,
    query_fields: &BTreeMap<FunctionName, ObjectFieldDefinition>,
    mutation_fields: &BTreeMap<ProcedureName, ObjectFieldDefinition>,
    root_type_names: &BTreeSet<TypeName>,
) -> Result<(), SchemaDefinitionError> {
    let validate_type_ref = |type_ref: &TypeRef, location: String| {
        validate_type_reference(type_ref, definitions, root_type_names, location)
    };

    for (type_name, type_def) in definitions {
        match type_def {
            TypeDef::Interface { possible_types, .. } => {
                if possible_types.is_empty() {
                    return Err(SchemaDefinitionError::InterfaceWithoutConcreteTypes(
                        type_name.clone(),
                    ));
                }
                for possible_type in possible_types {
                    match definitions.get(possible_type) {
                        Some(TypeDef::Object { .. }) => {}
                        Some(_) => {
                            return Err(SchemaDefinitionError::InterfaceConcreteTypeNotObject {
                                interface_name: type_name.clone(),
                                concrete_type: possible_type.clone(),
                            });
                        }
                        None => {
                            return Err(SchemaDefinitionError::InterfaceConcreteTypeNotFound {
                                interface_name: type_name.clone(),
                                concrete_type: possible_type.clone(),
                            });
                        }
                    }
                }
            }
            TypeDef::Union { members, .. } => {
                if members.is_empty() {
                    return Err(SchemaDefinitionError::UnionWithoutConcreteTypes(
                        type_name.clone(),
                    ));
                }
                for member in members {
                    match definitions.get(member) {
                        Some(TypeDef::Object { .. }) => {}
                        Some(_) => {
                            return Err(SchemaDefinitionError::UnionMemberNotObject {
                                union_name: type_name.clone(),
                                member_type: member.clone(),
                            });
                        }
                        None => {
                            return Err(SchemaDefinitionError::UnionMemberTypeNotFound {
                                union_name: type_name.clone(),
                                member_type: member.clone(),
                            });
                        }
                    }
                }
            }
            TypeDef::Scalar { .. }
            | TypeDef::Enum { .. }
            | TypeDef::Object { .. }
            | TypeDef::InputObject { .. } => {}
        }
    }

    for (query_field_name, field) in query_fields {
        validate_type_ref(
            &field.r#type,
            format!("query field {query_field_name} return type"),
        )?;
        for (argument_name, argument) in &field.arguments {
            validate_type_ref(
                &argument.r#type,
                format!("query field {query_field_name} argument {argument_name}"),
            )?;
        }
    }

    for (mutation_field_name, field) in mutation_fields {
        validate_type_ref(
            &field.r#type,
            format!("mutation field {mutation_field_name} return type"),
        )?;
        for (argument_name, argument) in &field.arguments {
            validate_type_ref(
                &argument.r#type,
                format!("mutation field {mutation_field_name} argument {argument_name}"),
            )?;
        }
    }

    for (type_name, type_definition) in definitions {
        match type_definition {
            TypeDef::Object { fields, .. } | TypeDef::Interface { fields, .. } => {
                for (field_name, field) in fields {
                    validate_type_ref(
                        &field.r#type,
                        format!("field {type_name}.{field_name} return type"),
                    )?;
                    for (argument_name, argument) in &field.arguments {
                        validate_type_ref(
                            &argument.r#type,
                            format!("field {type_name}.{field_name} argument {argument_name}"),
                        )?;
                    }
                }
            }
            TypeDef::InputObject { fields, .. } => {
                for (field_name, field) in fields {
                    validate_type_ref(
                        &field.r#type,
                        format!("input field {type_name}.{field_name}"),
                    )?;
                }
            }
            TypeDef::Scalar { .. } | TypeDef::Enum { .. } | TypeDef::Union { .. } => {}
        }
    }

    Ok(())
}

fn validate_type_reference(
    type_ref: &TypeRef,
    definitions: &BTreeMap<TypeName, TypeDef>,
    root_type_names: &BTreeSet<TypeName>,
    location: String,
) -> Result<(), SchemaDefinitionError> {
    match type_ref {
        TypeRef::Named(name) => {
            let type_name: TypeName = name.to_owned().into();
            if !definitions.contains_key(&type_name) && !root_type_names.contains(&type_name) {
                Err(SchemaDefinitionError::UnknownTypeReference {
                    referenced_type: type_name,
                    location,
                })
            } else {
                Ok(())
            }
        }
        TypeRef::List(inner) | TypeRef::NonNull(inner) => {
            validate_type_reference(inner, definitions, root_type_names, location)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SchemaDefinition, SchemaDefinitionError};
    use crate::config::{RequestConfig, ResponseConfig};

    #[test]
    fn interface_without_concrete_types_errors() {
        let schema_document = graphql_parser::parse_schema::<String>(
            r#"
            schema { query: Query }
            scalar ID
            type Query {
              account: PartyAccountRelationship
            }
            interface PartyAccountRelationship {
              relId: ID!
            }
            "#,
        )
        .expect("test schema should parse");

        let error = SchemaDefinition::new(
            &schema_document,
            &RequestConfig::default(),
            &ResponseConfig::default(),
        )
        .expect_err("schema should fail validation");

        assert!(matches!(
            error,
            SchemaDefinitionError::InterfaceWithoutConcreteTypes(_)
        ));
    }

    #[test]
    fn unknown_type_reference_errors() {
        let schema_document = graphql_parser::parse_schema::<String>(
            r#"
            schema { query: Query }
            type Query {
              account: UnknownType
            }
            "#,
        )
        .expect("test schema should parse");

        let error = SchemaDefinition::new(
            &schema_document,
            &RequestConfig::default(),
            &ResponseConfig::default(),
        )
        .expect_err("schema should fail validation");

        assert!(matches!(
            error,
            SchemaDefinitionError::UnknownTypeReference { .. }
        ));
    }
}
