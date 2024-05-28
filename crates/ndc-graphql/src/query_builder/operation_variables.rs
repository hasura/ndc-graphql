use std::collections::BTreeMap;

use graphql_parser::{
    query::{Type, Value, VariableDefinition},
    schema::InputValue,
    Pos,
};

pub struct OperationVariables<'c> {
    variables: BTreeMap<String, (serde_json::Value, &'c Type<'c, String>)>,
    variable_index: u32,
}

impl<'c> OperationVariables<'c> {
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
        input_value: &'c InputValue<'c, String>,
    ) -> Value<'c, String> {
        let name = format!("arg_{}_{}", self.variable_index, name);
        self.variable_index += 1;

        self.variables
            .insert(name.clone(), (value, &input_value.value_type));

        Value::Variable(name)
    }
    pub fn into_variable_definitions(
        self,
    ) -> (
        BTreeMap<String, serde_json::Value>,
        Vec<VariableDefinition<'c, String>>,
    ) {
        let (values, definitions) = self
            .variables
            .into_iter()
            .map(|(alias, (value, var_type))| {
                (
                    (alias.clone(), value),
                    VariableDefinition {
                        position: Pos { line: 0, column: 0 },
                        name: alias,
                        var_type: var_type.to_owned(),
                        default_value: None,
                    },
                )
            })
            .unzip();

        (values, definitions)
    }
}
