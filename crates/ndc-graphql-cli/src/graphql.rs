use std::{collections::BTreeMap, error::Error};

use common::{
    client::{execute_graphql, get_http_client},
    config::ConnectionConfig,
};
use graphql_parser::{
    query::{self, OperationDefinition, Selection, Type, Value},
    schema::{
        Definition, DirectiveDefinition, DirectiveLocation, Document, EnumType, EnumValue, Field,
        InputObjectType, InputValue, InterfaceType, ObjectType, ScalarType, SchemaDefinition,
        TypeDefinition, UnionType,
    },
    Pos,
};

use self::introspection::{InputTypeRef, Introspection, OutputTypeRef};
pub mod introspection;

pub async fn execute_graphql_introspection(
    connection: &ConnectionConfig,
) -> Result<graphql_client::Response<Introspection>, Box<dyn Error>> {
    let client = get_http_client(connection)?;

    let introspection_query = include_str!("./graphql/introspection_query.graphql");

    let (_, introspection) = execute_graphql::<Introspection>(
        introspection_query,
        BTreeMap::new(),
        &connection.endpoint,
        &connection.headers,
        &client,
        &vec![],
    )
    .await?;

    Ok(introspection)
}

/// graphql AST wants a position, but we don't actually use it.
fn pos() -> Pos {
    Pos { line: 0, column: 0 }
}

fn is_graphql_introspection_type(name: &str) -> bool {
    [
        "__Schema",
        "__Type",
        "__TypeKind",
        "__Field",
        "__InputValue",
        "__EnumValue",
        "__Directive",
        "__DirectiveLocation",
    ]
    .contains(&name)
}
pub fn schema_from_introspection(introspection: Introspection) -> Document<'static, String> {
    let mut definitions = vec![];

    definitions.push(Definition::SchemaDefinition(SchemaDefinition {
        position: pos(),
        directives: vec![], // todo
        query: Some(introspection.schema.query_type.name),
        mutation: introspection.schema.mutation_type.map(|t| t.name),
        subscription: introspection.schema.subscription_type.map(|t| t.name),
    }));

    for typedef in introspection.schema.types {
        let name = match &typedef {
            introspection::TypeDef::Scalar(introspection::Scalar { name, .. }) => name,
            introspection::TypeDef::Object(introspection::Object { name, .. }) => name,
            introspection::TypeDef::InputObject(introspection::InputObject { name, .. }) => name,
            introspection::TypeDef::Enum(introspection::Enum { name, .. }) => name,
            introspection::TypeDef::Interface(introspection::Interface { name, .. }) => name,
            introspection::TypeDef::Union(introspection::Union { name, .. }) => name,
        };
        if is_graphql_introspection_type(name) {
            continue;
        }
        let type_definition = match typedef {
            introspection::TypeDef::Scalar(scalar) => TypeDefinition::Scalar(ScalarType {
                position: pos(),
                description: scalar.description,
                name: scalar.name,
                directives: vec![],
            }),

            introspection::TypeDef::Object(object) => TypeDefinition::Object(ObjectType {
                position: pos(),
                description: object.description,
                name: object.name,
                implements_interfaces: object.interfaces.into_iter().map(|i| i.name).collect(),
                directives: vec![],
                fields: object
                    .fields
                    .into_iter()
                    .map(|field| Field {
                        position: pos(),
                        description: field.description,
                        name: field.name,
                        arguments: field
                            .args
                            .into_iter()
                            .map(|arg| InputValue {
                                position: pos(),
                                description: arg.description,
                                name: arg.name,
                                value_type: input_type(arg.r#type),
                                default_value: arg.default_value.map(parse_value),
                                directives: vec![],
                            })
                            .collect(),
                        field_type: output_type(field.r#type),
                        directives: vec![],
                    })
                    .collect(),
            }),
            introspection::TypeDef::InputObject(input) => {
                TypeDefinition::InputObject(InputObjectType {
                    position: pos(),
                    description: input.description,
                    name: input.name,
                    directives: vec![],
                    fields: input
                        .input_fields
                        .into_iter()
                        .map(|field| InputValue {
                            position: pos(),
                            description: field.description,
                            name: field.name,
                            value_type: input_type(field.r#type),
                            default_value: field.default_value.map(parse_value),
                            directives: vec![],
                        })
                        .collect(),
                })
            }
            introspection::TypeDef::Enum(enum_def) => TypeDefinition::Enum(EnumType {
                position: pos(),
                description: enum_def.description,
                name: enum_def.name,
                directives: vec![],
                values: enum_def
                    .enum_values
                    .into_iter()
                    .map(|val| EnumValue {
                        position: pos(),
                        description: val.description,
                        name: val.name,
                        directives: vec![],
                    })
                    .collect(),
            }),
            introspection::TypeDef::Interface(interface) => {
                TypeDefinition::Interface(InterfaceType {
                    position: pos(),
                    description: interface.description,
                    name: interface.name,
                    implements_interfaces: interface
                        .interfaces
                        .unwrap_or_default()
                        .into_iter()
                        .map(|i| i.name)
                        .collect(),
                    directives: vec![],
                    fields: interface
                        .fields
                        .into_iter()
                        .map(|field| Field {
                            position: pos(),
                            description: field.description,
                            name: field.name,
                            arguments: field
                                .args
                                .into_iter()
                                .map(|arg| InputValue {
                                    position: pos(),
                                    description: arg.description,
                                    name: arg.name,
                                    value_type: input_type(arg.r#type),
                                    default_value: arg.default_value.map(parse_value),
                                    directives: vec![],
                                })
                                .collect(),
                            field_type: output_type(field.r#type),
                            directives: vec![],
                        })
                        .collect(),
                })
            }
            introspection::TypeDef::Union(union) => TypeDefinition::Union(UnionType {
                position: pos(),
                description: union.description,
                name: union.name,
                directives: vec![],
                types: union.possible_types.into_iter().map(|t| t.name).collect(),
            }),
        };

        definitions.push(Definition::TypeDefinition(type_definition));
    }

    for directive in introspection.schema.directives {
        definitions.push(Definition::DirectiveDefinition(DirectiveDefinition {
            position: pos(),
            description: directive.description,
            name: directive.name,
            arguments: directive
                .args
                .into_iter()
                .map(|arg| InputValue {
                    position: pos(),
                    description: arg.description,
                    name: arg.name,
                    value_type: input_type(arg.r#type),
                    default_value: arg.default_value.map(Value::String),
                    directives: vec![],
                })
                .collect(),
            repeatable: false,
            locations: directive
                .locations
                .iter()
                .map(|loc| match loc {
                    introspection::DirectiveLocation::Query => DirectiveLocation::Query,
                    introspection::DirectiveLocation::Mutation => DirectiveLocation::Mutation,
                    introspection::DirectiveLocation::Subscription => {
                        DirectiveLocation::Subscription
                    }
                    introspection::DirectiveLocation::Field => DirectiveLocation::Field,
                    introspection::DirectiveLocation::FragmentDefinition => {
                        DirectiveLocation::FragmentDefinition
                    }
                    introspection::DirectiveLocation::FragmentSpread => {
                        DirectiveLocation::FragmentSpread
                    }
                    introspection::DirectiveLocation::InlineFragment => {
                        DirectiveLocation::InlineFragment
                    }
                    introspection::DirectiveLocation::Schema => DirectiveLocation::Schema,
                    introspection::DirectiveLocation::Scalar => DirectiveLocation::Scalar,
                    introspection::DirectiveLocation::Object => DirectiveLocation::Object,
                    introspection::DirectiveLocation::FieldDefinition => {
                        DirectiveLocation::FieldDefinition
                    }
                    introspection::DirectiveLocation::ArgumentDefinition => {
                        DirectiveLocation::ArgumentDefinition
                    }
                    introspection::DirectiveLocation::Interface => DirectiveLocation::Interface,
                    introspection::DirectiveLocation::Union => DirectiveLocation::Union,
                    introspection::DirectiveLocation::Enum => DirectiveLocation::Enum,
                    introspection::DirectiveLocation::EnumValue => DirectiveLocation::EnumValue,
                    introspection::DirectiveLocation::InputObject => DirectiveLocation::InputObject,
                    introspection::DirectiveLocation::InputFieldDefinition => {
                        DirectiveLocation::InputFieldDefinition
                    }
                })
                .collect(),
        }))
    }

    graphql_parser::schema::Document { definitions }
}

fn parse_value(value: String) -> Value<'static, String> {
    // to parse a value using graphql parser, we build a dummy query
    // this is a hack but it works, and this is not performance critical
    let query_string = format!(r#"query {{ field(value: {value}) }}"#);

    // We've just built the query, so we can make some assumptions about the shape of the resulting AST.
    // We're also assuming that the value can be parsed successfully.
    // Since this is the CLI plugin, we can pannic if our assumptions are incorrect
    let document = graphql_parser::parse_query::<'_, String>(&query_string)
        .expect("Default value should be a valid graphql value");
    let operation = &document.definitions[0];
    let query = match operation {
        query::Definition::Operation(operation) => match operation {
            OperationDefinition::Query(query) => query,
            _ => panic!("Expected Query Operation Definition"),
        },
        _ => panic!("Expected Operation Definition"),
    };
    let field = match &query.selection_set.items[0] {
        Selection::Field(field) => field,
        _ => panic!("Expected field selection"),
    };
    let argument = &field.arguments[0];
    let (_name, value) = argument;

    value.into_static().to_owned()
}

#[test]
fn test_parse_value() {
    let values = vec![
        "[ENUM_1, ENUM_2]",
        "1.234",
        r#""String""#,
        r#"{object: "property"}"#,
    ];

    for value in values {
        let parsed_value = parse_value(value.to_string());
        assert_eq!(
            value,
            parsed_value.to_string(),
            "GraphQL value {value} should be parsed correctly"
        )
    }
}

fn input_type(input: InputTypeRef) -> Type<'static, String> {
    match input {
        InputTypeRef::Scalar(named)
        | InputTypeRef::InputObject(named)
        | InputTypeRef::Enum(named) => Type::NamedType(named.name),
        InputTypeRef::List(list) => Type::ListType(Box::new(input_type(*list.of_type))),
        InputTypeRef::NonNull(non_null) => {
            Type::NonNullType(Box::new(input_type(*non_null.of_type)))
        }
    }
}
fn output_type(output: OutputTypeRef) -> Type<'static, String> {
    match output {
        OutputTypeRef::Scalar(named)
        | OutputTypeRef::Object(named)
        | OutputTypeRef::Enum(named)
        | OutputTypeRef::Interface(named)
        | OutputTypeRef::Union(named) => Type::NamedType(named.name),
        OutputTypeRef::List(list) => Type::ListType(Box::new(output_type(*list.of_type))),
        OutputTypeRef::NonNull(non_null) => {
            Type::NonNullType(Box::new(output_type(*non_null.of_type)))
        }
    }
}
