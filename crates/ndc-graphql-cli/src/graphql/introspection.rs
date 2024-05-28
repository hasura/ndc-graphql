// use super::introspection_query;
use serde::{Deserialize, Serialize};
#[derive(Debug, Serialize, Deserialize)]
pub struct Introspection {
    #[serde(rename = "__schema")]
    pub schema: Schema,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Schema {
    pub query_type: NamedTypeRef,
    pub mutation_type: Option<NamedTypeRef>,
    pub subscription_type: Option<NamedTypeRef>,
    pub types: Vec<TypeDef>,
    pub directives: Vec<Directive>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub struct Field {
    pub name: String,
    pub description: Option<String>,
    pub args: Vec<InputValue>,
    #[serde(rename = "type")]
    pub r#type: OutputTypeRef,
    pub is_deprecated: bool,
    pub deprecation_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputValue {
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub r#type: InputTypeRef,
    pub default_value: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnumValue {
    pub name: String,
    pub description: Option<String>,
    pub is_deprecated: bool,
    pub deprecation_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Directive {
    pub name: String,
    pub description: Option<String>,
    pub locations: Vec<DirectiveLocation>,
    pub args: Vec<InputValue>,
    pub is_repeatable: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DirectiveLocation {
    Query,
    Mutation,
    Subscription,
    Field,
    FragmentDefinition,
    FragmentSpread,
    InlineFragment,
    Schema,
    Scalar,
    Object,
    FieldDefinition,
    ArgumentDefinition,
    Interface,
    Union,
    Enum,
    EnumValue,
    InputObject,
    InputFieldDefinition,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scalar {
    pub name: String,
    pub description: Option<String>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Object {
    pub name: String,
    pub description: Option<String>,
    pub fields: Vec<Field>,
    pub interfaces: Vec<NamedTypeRef>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Interface {
    pub name: String,
    pub description: Option<String>,
    pub fields: Vec<Field>,
    pub interfaces: Vec<NamedTypeRef>,
    pub possible_types: Vec<NamedTypeRef>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Union {
    pub name: String,
    pub description: Option<String>,
    pub possible_types: Vec<NamedTypeRef>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Enum {
    pub name: String,
    pub description: Option<String>,
    pub enum_values: Vec<EnumValue>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InputObject {
    pub name: String,
    pub description: Option<String>,
    pub input_fields: Vec<InputValue>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct List<T> {
    pub of_type: Box<T>,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NonNull<T> {
    pub of_type: Box<T>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TypeDef {
    Scalar(Scalar),
    Object(Object),
    InputObject(InputObject),
    Enum(Enum),
    Interface(Interface),
    Union(Union),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OutputTypeRef {
    Scalar(NamedTypeRef),
    Object(NamedTypeRef),
    Enum(NamedTypeRef),
    Interface(NamedTypeRef),
    Union(NamedTypeRef),
    List(List<OutputTypeRef>),
    NonNull(NonNull<OutputTypeRef>),
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InputTypeRef {
    Scalar(NamedTypeRef),
    InputObject(NamedTypeRef),
    Enum(NamedTypeRef),
    List(List<InputTypeRef>),
    NonNull(NonNull<InputTypeRef>),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ObjectTypeRef {
    Object(NamedTypeRef),
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InterfaceTypeRef {
    Interface(NamedTypeRef),
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NamedTypeRef {
    pub name: String,
}
