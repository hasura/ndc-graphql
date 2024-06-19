use std::collections::BTreeMap;

use common::schema::TypeRef;
use graphql_parser::{
    query::{Type, Value, VariableDefinition},
    Pos,
};

pub struct OperationVariables {
    variables: BTreeMap<String, (serde_json::Value, TypeRef)>,
    variable_index: u32,
}

impl<'c> OperationVariables {
    pub fn new() -> Self {
        Self {
            variables: BTreeMap::new(),
            variable_index: 1,
        }
    }
    pub fn insert(
        &mut self,
        name: &str,
        value: serde_json::Value,
        r#type: &TypeRef,
    ) -> Value<'c, String> {
        let name = format!("arg_{}_{}", self.variable_index, name);
        self.variable_index += 1;

        self.variables
            .insert(name.clone(), (value, r#type.to_owned()));

        Value::Variable(name)
    }
    pub fn into_variable_definitions(
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
            .variables
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
