use common::config::schema::{TypeDef, TypeRef};
use ndc_sdk::models::TypeName;
use std::collections::BTreeMap;

/// Coerce a `serde_json::Value` to match the expected GraphQL scalar type.
///
/// The v3-engine serializes GraphQL values using `value.as_json()`, which loses
/// type information (e.g. GraphQL `ID` values become JSON strings even when the
/// input was an integer). This function walks the value recursively according to
/// the upstream GraphQL type system and coerces leaf scalar values to match.
pub fn coerce_value(
    value: serde_json::Value,
    type_ref: &TypeRef,
    definitions: &BTreeMap<TypeName, TypeDef>,
) -> serde_json::Value {
    match value {
        serde_json::Value::Null => serde_json::Value::Null,
        _ => match type_ref {
            TypeRef::NonNull(inner) => coerce_value(value, inner, definitions),
            TypeRef::List(inner) => match value {
                serde_json::Value::Array(items) => serde_json::Value::Array(
                    items
                        .into_iter()
                        .map(|item| coerce_value(item, inner, definitions))
                        .collect(),
                ),
                _ => coerce_value(value, inner, definitions),
            },
            TypeRef::Named(name) => coerce_named(value, name, definitions),
        },
    }
}

fn coerce_named(
    value: serde_json::Value,
    type_name: &str,
    definitions: &BTreeMap<TypeName, TypeDef>,
) -> serde_json::Value {
    match type_name {
        "Int" => coerce_to_int(value),
        "Float" => coerce_to_float(value),
        "Boolean" => coerce_to_boolean(value),
        "String" => coerce_to_string(value),
        "ID" => value, // GraphQL ID accepts both strings and integers
        _ => {
            let type_key: TypeName = type_name.to_owned().into();
            match definitions.get(&type_key) {
                Some(TypeDef::InputObject { fields, .. }) => coerce_input_object(value, fields, definitions),
                // enums are already strings; custom scalars and output types pass through
                _ => value,
            }
        }
    }
}

fn coerce_input_object(
    value: serde_json::Value,
    fields: &BTreeMap<ndc_sdk::models::FieldName, common::config::schema::InputObjectFieldDefinition>,
    definitions: &BTreeMap<TypeName, TypeDef>,
) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let coerced: serde_json::Map<String, serde_json::Value> = map
                .into_iter()
                .map(|(key, val)| {
                    let field_name: ndc_sdk::models::FieldName = key.clone().into();
                    let coerced_val = match fields.get(&field_name) {
                        Some(field_def) => coerce_value(val, &field_def.r#type, definitions),
                        None => val, // unknown field: pass through
                    };
                    (key, coerced_val)
                })
                .collect();
            serde_json::Value::Object(coerced)
        }
        _ => value,
    }
}

fn coerce_to_int(value: serde_json::Value) -> serde_json::Value {
    match &value {
        serde_json::Value::String(s) => match s.parse::<i64>() {
            Ok(n) => serde_json::Value::Number(n.into()),
            Err(_) => value,
        },
        _ => value,
    }
}

fn coerce_to_float(value: serde_json::Value) -> serde_json::Value {
    match &value {
        serde_json::Value::String(s) => s
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(serde_json::Value::Number)
            .unwrap_or(value),
        serde_json::Value::Number(n) => {
            // Convert integer to float (e.g. 1 -> 1.0)
            #[allow(clippy::cast_precision_loss)]
            if let Some(i) = n.as_i64() {
                serde_json::Number::from_f64(i as f64)
                    .map(serde_json::Value::Number)
                    .unwrap_or(value)
            } else {
                value
            }
        }
        _ => value,
    }
}

fn coerce_to_boolean(value: serde_json::Value) -> serde_json::Value {
    match &value {
        serde_json::Value::String(s) => match s.as_str() {
            "true" => serde_json::Value::Bool(true),
            "false" => serde_json::Value::Bool(false),
            _ => value,
        },
        _ => value,
    }
}

fn coerce_to_string(value: serde_json::Value) -> serde_json::Value {
    match &value {
        serde_json::Value::Number(n) => serde_json::Value::String(n.to_string()),
        serde_json::Value::Bool(b) => serde_json::Value::String(b.to_string()),
        _ => value,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn empty_definitions() -> BTreeMap<TypeName, TypeDef> {
        BTreeMap::new()
    }

    #[test]
    fn coerce_string_to_int() {
        let result = coerce_value(json!("42"), &TypeRef::Named("Int".to_string()), &empty_definitions());
        assert_eq!(result, json!(42));
    }

    #[test]
    fn coerce_int_stays_int() {
        let result = coerce_value(json!(42), &TypeRef::Named("Int".to_string()), &empty_definitions());
        assert_eq!(result, json!(42));
    }

    #[test]
    fn coerce_string_to_float() {
        let result = coerce_value(json!("1.5"), &TypeRef::Named("Float".to_string()), &empty_definitions());
        assert_eq!(result, json!(1.5));
    }

    #[test]
    fn coerce_int_to_float() {
        let result = coerce_value(json!(1), &TypeRef::Named("Float".to_string()), &empty_definitions());
        assert_eq!(result, json!(1.0));
    }

    #[test]
    fn coerce_string_to_boolean() {
        let result = coerce_value(json!("true"), &TypeRef::Named("Boolean".to_string()), &empty_definitions());
        assert_eq!(result, json!(true));

        let result = coerce_value(json!("false"), &TypeRef::Named("Boolean".to_string()), &empty_definitions());
        assert_eq!(result, json!(false));
    }

    #[test]
    fn coerce_number_to_string() {
        let result = coerce_value(json!(42), &TypeRef::Named("String".to_string()), &empty_definitions());
        assert_eq!(result, json!("42"));
    }

    #[test]
    fn coerce_id_passes_through() {
        let result = coerce_value(json!(42), &TypeRef::Named("ID".to_string()), &empty_definitions());
        assert_eq!(result, json!(42));

        let result = coerce_value(json!("42"), &TypeRef::Named("ID".to_string()), &empty_definitions());
        assert_eq!(result, json!("42"));
    }

    #[test]
    fn coerce_null_passes_through() {
        let result = coerce_value(json!(null), &TypeRef::Named("Int".to_string()), &empty_definitions());
        assert_eq!(result, json!(null));
    }

    #[test]
    fn coerce_nonnull_unwraps() {
        let type_ref = TypeRef::NonNull(Box::new(TypeRef::Named("Int".to_string())));
        let result = coerce_value(json!("42"), &type_ref, &empty_definitions());
        assert_eq!(result, json!(42));
    }

    #[test]
    fn coerce_list_elements() {
        let type_ref = TypeRef::List(Box::new(TypeRef::Named("Int".to_string())));
        let result = coerce_value(json!(["1", "2", 3]), &type_ref, &empty_definitions());
        assert_eq!(result, json!([1, 2, 3]));
    }

    #[test]
    fn coerce_input_object_fields() {
        let mut fields = BTreeMap::new();
        fields.insert(
            "_gt".to_string().into(),
            common::config::schema::InputObjectFieldDefinition {
                r#type: TypeRef::Named("Int".to_string()),
                description: None,
            },
        );

        let mut definitions = BTreeMap::new();
        definitions.insert(
            "Int_comparison_exp".to_string().into(),
            TypeDef::InputObject {
                fields,
                description: None,
            },
        );

        let type_ref = TypeRef::Named("Int_comparison_exp".to_string());
        let result = coerce_value(json!({"_gt": "5"}), &type_ref, &definitions);
        assert_eq!(result, json!({"_gt": 5}));
    }

    #[test]
    fn coerce_custom_scalar_passes_through() {
        let mut definitions = BTreeMap::new();
        definitions.insert(
            "DateTime".to_string().into(),
            TypeDef::Scalar { description: None },
        );

        let result = coerce_value(
            json!("2024-01-01"),
            &TypeRef::Named("DateTime".to_string()),
            &definitions,
        );
        assert_eq!(result, json!("2024-01-01"));
    }

    #[test]
    fn coerce_unparseable_string_passes_through() {
        let result = coerce_value(json!("not_a_number"), &TypeRef::Named("Int".to_string()), &empty_definitions());
        assert_eq!(result, json!("not_a_number"));
    }
}
