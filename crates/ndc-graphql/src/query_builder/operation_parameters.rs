use std::collections::BTreeMap;

use common::schema::TypeRef;
use graphql_parser::{
    query::{Type, Value, VariableDefinition},
    Pos,
};

pub struct OperationParameters {
    namespace: String,
    parameters: BTreeMap<String, (serde_json::Value, TypeRef)>,
    parameter_index: u32,
}

impl<'c> OperationParameters {
    pub fn new<S: Into<String>>(namespace: S) -> Self {
        Self {
            namespace: namespace.into(),
            parameters: BTreeMap::new(),
            parameter_index: 1,
        }
    }
    pub fn insert(
        &mut self,
        name: &str,
        value: serde_json::Value,
        r#type: &TypeRef,
    ) -> Value<'c, String> {
        let name = format!("{}arg_{}_{}", self.namespace, self.parameter_index, name);
        self.parameter_index += 1;

        self.parameters
            .insert(name.clone(), (value, r#type.to_owned()));

        Value::Variable(name)
    }
    pub fn into_parameter_definitions(
        self,
    ) -> (
        BTreeMap<String, serde_json::Value>,
        Vec<VariableDefinition<'c, String>>,
    ) {
        fn typeref_to_type<'c>(typeref: TypeRef) -> Type<'c, String> {
            match typeref {
                TypeRef::Named(name) => Type::NamedType(name),
                TypeRef::List(underlying) => Type::ListType(Box::new(typeref_to_type(*underlying))),
                TypeRef::NonNull(underlying) => {
                    Type::NonNullType(Box::new(typeref_to_type(*underlying)))
                }
            }
        }
        let (values, definitions) = self
            .parameters
            .into_iter()
            .map(|(alias, (value, typeref))| {
                (
                    (alias.clone(), value),
                    VariableDefinition {
                        position: Pos { line: 0, column: 0 },
                        name: alias,
                        var_type: typeref_to_type(typeref),
                        default_value: None,
                    },
                )
            })
            .unzip();

        (values, definitions)
    }
}
