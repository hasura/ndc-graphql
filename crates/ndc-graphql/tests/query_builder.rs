use common::capabilities::capabilities_response;
use common::config::ServerConfig;
use common::{config::config_file::ServerConfigFile, schema_response::schema_response};
use insta::{assert_json_snapshot, assert_snapshot, assert_yaml_snapshot, glob};
use ndc_graphql::{
    connector::setup::GraphQLConnectorSetup,
    query_builder::{build_mutation_document, build_query_document, error::QueryBuilderError},
};
use ndc_sdk::models::{self, Type};
use schemars::schema_for;
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::PathBuf,
};

#[tokio::test]
#[ignore]
async fn update_json_schema() {
    for config in ["config-1", "config-2", "config-3", "config-4"] {
        fs::write(
            format!("./tests/{config}/queries/_query_request.schema.json"),
            serde_json::to_string_pretty(&schema_for!(models::QueryRequest))
                .expect("Should serialize schema to json"),
        )
        .expect("Should be able to write out schema file");
        fs::write(
            format!("./tests/{config}/mutations/_mutation_request.schema.json"),
            serde_json::to_string_pretty(&schema_for!(models::MutationRequest))
                .expect("Should serialize schema to json"),
        )
        .expect("Should be able to write out schema file");
        fs::write(
            format!("./tests/{config}/configuration/configuration.schema.json"),
            serde_json::to_string_pretty(&schema_for!(ServerConfigFile))
                .expect("Should serialize schema to json"),
        )
        .expect("Should be able to write out schema file");
    }
}

async fn read_configuration(config: &str) -> ServerConfig {
    let configuration_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join(config)
        .join("configuration");
    let env = HashMap::from_iter(vec![
        ("GRAPHQL_ENDPOINT".to_owned(), String::new()),
        ("GRAPHQL_ENDPOINT_SECRET".to_owned(), String::new()),
    ]);
    GraphQLConnectorSetup::new(env)
        .read_configuration(configuration_dir)
        .await
        .expect("Should sucessfully read configuration")
}

// We use insta for snapshot testing
// Install it with `cargo install cargo-insta`
// The usual workflow is to run `cargo insta test`, then `cargo insta review`
// For more info, see insta docs: https://insta.rs/

#[tokio::test]
async fn test_build_graphql_query() {
    for config in ["config-1", "config-2", "config-3"] {
        let configuration = read_configuration(config).await;

        glob!(format!("./{config}/queries"), "*.request.json", |path| {
            let request = fs::read_to_string(path).expect("Should be able to read file");
            let request: models::QueryRequest =
                serde_json::from_str(&request).expect("Should be valid request json");
            let operation = build_query_document(&request, &configuration)
                .expect("Should sucessfully build query document");

            assert_snapshot!("Query String", operation.query);
            assert_json_snapshot!("Variables", operation.variables);
            assert_yaml_snapshot!("Headers", operation.headers);
        });
    }
}

#[tokio::test]
async fn test_build_graphql_query_polymorphic() {
    let configuration = read_configuration("config-4").await;
    let request = fs::read_to_string("./tests/config-4/queries/01_polymorphic_query.request.json")
        .expect("Should be able to read file");
    let request: models::QueryRequest =
        serde_json::from_str(&request).expect("Should be valid request json");
    let operation = build_query_document(&request, &configuration)
        .expect("Should sucessfully build query document");

    assert_snapshot!("Polymorphic Query String", operation.query);
    assert_json_snapshot!("Polymorphic Variables", operation.variables);
    assert_yaml_snapshot!("Polymorphic Headers", operation.headers);
}

#[tokio::test]
async fn test_build_graphql_mutation() {
    for config in ["config-1", "config-2", "config-3"] {
        let configuration = read_configuration(config).await;

        glob!(format!("./{config}/mutations"), "*.request.json", |path| {
            let request = fs::read_to_string(path).expect("Should be able to read file");
            let request: models::MutationRequest =
                serde_json::from_str(&request).expect("Should be valid request json");
            let operation = build_mutation_document(&request, &configuration)
                .expect("Should sucessfully build query document");

            assert_snapshot!("Query String", operation.query);
            assert_json_snapshot!("Variables", operation.variables);
            assert_yaml_snapshot!("Headers", operation.headers);
        });
    }
}

#[tokio::test]
async fn test_generated_schema() {
    for config in ["config-1", "config-2", "config-3", "config-4"] {
        let configuration = read_configuration(config).await;
        let schema = schema_response(
            &configuration.schema,
            &configuration.request,
            &configuration.response,
        );
        assert_schema_references_resolve(&schema);
        assert_yaml_snapshot!(format!("{config} NDC Schema"), schema);
    }
}

#[tokio::test]
async fn test_polymorphic_selection_errors() {
    let configuration = read_configuration("config-4").await;

    let unknown_variant =
        query_request_with_polymorphic_field("on_DoesNotExist", None, nested_object_field("relId"));
    assert!(
        matches!(
            build_query_document(&unknown_variant, &configuration),
            Err(QueryBuilderError::PolymorphicFieldNotFound { .. })
        ),
        "unknown variant fields should fail with PolymorphicFieldNotFound"
    );

    let missing_selection =
        query_request_with_polymorphic_field("on_PersonRelationship", None, None);
    assert!(
        matches!(
            build_query_document(&missing_selection, &configuration),
            Err(QueryBuilderError::PolymorphicFieldMissingSelection { .. })
        ),
        "variant fields without nested selection should fail with PolymorphicFieldMissingSelection"
    );

    let variant_with_arguments = query_request_with_polymorphic_field(
        "on_PersonRelationship",
        Some(BTreeMap::from([(
            "ignored".into(),
            models::Argument::Literal {
                value: serde_json::json!(1),
            },
        )])),
        nested_object_field("relId"),
    );
    assert!(
        matches!(
            build_query_document(&variant_with_arguments, &configuration),
            Err(QueryBuilderError::PolymorphicFieldArgumentsNotSupported { .. })
        ),
        "variant fields with arguments should fail with PolymorphicFieldArgumentsNotSupported"
    );

    let typename_with_arguments = query_request_with_polymorphic_field(
        "__typename",
        Some(BTreeMap::from([(
            "ignored".into(),
            models::Argument::Literal {
                value: serde_json::json!(1),
            },
        )])),
        None,
    );
    assert!(
        matches!(
            build_query_document(&typename_with_arguments, &configuration),
            Err(QueryBuilderError::TypenameArgumentsNotSupported { .. })
        ),
        "__typename with arguments should fail with TypenameArgumentsNotSupported"
    );

    let common_field_under_variant = query_request_with_polymorphic_field(
        "on_PersonRelationship",
        None,
        nested_object_field("relId"),
    );
    assert!(
        matches!(
            build_query_document(&common_field_under_variant, &configuration),
            Err(QueryBuilderError::ObjectFieldNotFound { .. })
        ),
        "common interface fields should not be selectable under on_<Type> variants"
    );
}

#[tokio::test]
async fn test_polymorphic_selection_alias_supported() {
    let configuration = read_configuration("config-4").await;

    let mut request = query_request_with_polymorphic_field(
        "on_PersonRelationship",
        None,
        nested_object_field("personName"),
    );
    rename_polymorphic_variant_alias(&mut request, "on_PersonRelationship", "onPerson");

    let operation = build_query_document(&request, &configuration)
        .expect("aliased polymorphic selections should build");

    assert!(
        operation.query.contains("... on PersonRelationship"),
        "expected inline fragment for aliased polymorphic field"
    );
}

#[tokio::test]
async fn test_interface_common_field_supported() {
    let configuration = read_configuration("config-4").await;
    let request = query_request_with_polymorphic_field("relId", None, None);

    let operation = build_query_document(&request, &configuration)
        .expect("interface common field should build");

    assert!(
        operation.query.contains("relationship {\n      relId"),
        "expected direct interface field selection"
    );
}

#[test]
fn test_capabilities() {
    assert_yaml_snapshot!("Capabilities", capabilities_response());
}

#[test]
fn configuration_schema() {
    assert_snapshot!(
        "Configuration JSON Schema",
        serde_json::to_string_pretty(&schema_for!(ServerConfigFile))
            .expect("Should serialize schema to json")
    );
}

fn assert_schema_references_resolve(schema: &models::SchemaResponse) {
    for (object_name, object_type) in &schema.object_types {
        for (field_name, field) in &object_type.fields {
            assert_type_resolves(
                &field.r#type,
                &schema.object_types,
                &schema.scalar_types,
                &format!("{object_name}.{field_name}"),
            );
        }
    }

    for function in &schema.functions {
        for (argument_name, argument) in &function.arguments {
            assert_type_resolves(
                &argument.argument_type,
                &schema.object_types,
                &schema.scalar_types,
                &format!("function {} argument {}", function.name, argument_name),
            );
        }
        assert_type_resolves(
            &function.result_type,
            &schema.object_types,
            &schema.scalar_types,
            &format!("function {} result", function.name),
        );
    }

    for procedure in &schema.procedures {
        for (argument_name, argument) in &procedure.arguments {
            assert_type_resolves(
                &argument.argument_type,
                &schema.object_types,
                &schema.scalar_types,
                &format!("procedure {} argument {}", procedure.name, argument_name),
            );
        }
        assert_type_resolves(
            &procedure.result_type,
            &schema.object_types,
            &schema.scalar_types,
            &format!("procedure {} result", procedure.name),
        );
    }
}

fn assert_type_resolves(
    ndc_type: &Type,
    object_types: &BTreeMap<models::ObjectTypeName, models::ObjectType>,
    scalar_types: &BTreeMap<models::ScalarTypeName, models::ScalarType>,
    context: &str,
) {
    match ndc_type {
        Type::Named { name } => assert!(
            object_types.contains_key(name.as_str()) || scalar_types.contains_key(name.as_str()),
            "unresolved NDC type {name} referenced by {context}"
        ),
        Type::Nullable { underlying_type } => {
            assert_type_resolves(underlying_type, object_types, scalar_types, context)
        }
        Type::Array { element_type } => {
            assert_type_resolves(element_type, object_types, scalar_types, context)
        }
        Type::Predicate { object_type_name } => assert!(
            object_types.contains_key(object_type_name),
            "unresolved predicate object type {object_type_name} referenced by {context}"
        ),
    }
}

fn query_request_with_polymorphic_field(
    field_name: &str,
    arguments: Option<BTreeMap<models::ArgumentName, models::Argument>>,
    fields: Option<models::NestedField>,
) -> models::QueryRequest {
    let mut relationship_fields = indexmap::IndexMap::new();
    relationship_fields.insert(
        field_name.into(),
        models::Field::Column {
            column: field_name.into(),
            fields,
            arguments: arguments.unwrap_or_default(),
        },
    );

    models::QueryRequest {
        collection: "getAccountById".into(),
        query: models::Query {
            aggregates: None,
            fields: Some(indexmap::IndexMap::from([(
                "__value".into(),
                models::Field::Column {
                    column: "__value".into(),
                    fields: Some(models::NestedField::Object(models::NestedObject {
                        fields: indexmap::IndexMap::from([(
                            "relationship".into(),
                            models::Field::Column {
                                column: "relationship".into(),
                                fields: Some(models::NestedField::Object(models::NestedObject {
                                    fields: relationship_fields,
                                })),
                                arguments: BTreeMap::new(),
                            },
                        )]),
                    })),
                    arguments: BTreeMap::new(),
                },
            )])),
            limit: None,
            offset: None,
            order_by: None,
            predicate: None,
            groups: None,
        },
        arguments: BTreeMap::from([(
            "id".into(),
            models::Argument::Literal {
                value: serde_json::json!("acct-1"),
            },
        )]),
        collection_relationships: BTreeMap::new(),
        variables: None,
        request_arguments: None,
    }
}

fn nested_object_field(field_name: &str) -> Option<models::NestedField> {
    Some(models::NestedField::Object(models::NestedObject {
        fields: indexmap::IndexMap::from([(
            field_name.into(),
            models::Field::Column {
                column: field_name.into(),
                fields: None,
                arguments: BTreeMap::new(),
            },
        )]),
    }))
}

fn rename_polymorphic_variant_alias(
    request: &mut models::QueryRequest,
    old_alias: &str,
    new_alias: &str,
) {
    let root_field = request
        .query
        .fields
        .as_mut()
        .and_then(|fields| fields.get_mut("__value"))
        .expect("request should contain __value");

    let models::Field::Column {
        fields: Some(models::NestedField::Object(root_object)),
        ..
    } = root_field
    else {
        panic!("__value should contain an object selection");
    };

    let relationship_field = root_object
        .fields
        .get_mut("relationship")
        .expect("request should contain relationship field");

    let models::Field::Column {
        fields: Some(models::NestedField::Object(relationship_object)),
        ..
    } = relationship_field
    else {
        panic!("relationship field should contain an object selection");
    };

    let old_alias: models::FieldName = old_alias.into();
    let new_alias: models::FieldName = new_alias.into();

    let field = relationship_object
        .fields
        .swap_remove(&old_alias)
        .expect("request should contain old polymorphic alias");
    relationship_object.fields.insert(new_alias, field);
}
