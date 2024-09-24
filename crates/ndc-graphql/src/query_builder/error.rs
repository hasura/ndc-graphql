use std::fmt::Display;

use ndc_sdk::connector::{ExplainError, MutationError, QueryError};

#[derive(Debug)]
pub enum QueryBuilderError {
    SchemaDefinitionNotFound,
    ObjectTypeNotFound(String),
    InputObjectTypeNotFound(String),
    NoRequesQueryFields,
    NoQueryType,
    NoMutationType,
    NotSupported(String),
    QueryFieldNotFound {
        field: String,
    },
    MutationFieldNotFound {
        field: String,
    },
    ObjectFieldNotFound {
        object: String,
        field: String,
    },
    InputObjectFieldNotFound {
        input_object: String,
        field: String,
    },
    ArgumentNotFound {
        object: String,
        field: String,
        argument: String,
    },
    MisshapenHeadersArgument(serde_json::Value),
    Unexpected(String),
    MissingVariable(String),
}

impl std::error::Error for QueryBuilderError {}

impl From<QueryBuilderError> for QueryError {
    fn from(value: QueryBuilderError) -> Self {
        QueryError::new_invalid_request(&value)
    }
}
impl From<QueryBuilderError> for MutationError {
    fn from(value: QueryBuilderError) -> Self {
        MutationError::new_invalid_request(&value)
    }
}
impl From<QueryBuilderError> for ExplainError {
    fn from(value: QueryBuilderError) -> Self {
        ExplainError::new_invalid_request(&value)
    }
}

impl Display for QueryBuilderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            QueryBuilderError::SchemaDefinitionNotFound => {
                write!(f, "Schema definition not found in configuration")
            }
            QueryBuilderError::ObjectTypeNotFound(obj) => {
                write!(f, "Object Type {obj} not found in configuration")
            }
            QueryBuilderError::InputObjectTypeNotFound(input) => {
                write!(f, "Input Object Type {input} not found in configuration")
            }
            QueryBuilderError::NoRequesQueryFields => {
                write!(f, "Misshapen request: no fields for query")
            }
            QueryBuilderError::NoQueryType => write!(f, "No query type in schema definition"),
            QueryBuilderError::NoMutationType => write!(f, "No mutation type in schema definition"),
            QueryBuilderError::NotSupported(feature) => {
                write!(f, "Feature not supported: {feature}")
            }
            QueryBuilderError::ObjectFieldNotFound { object, field } => {
                write!(f, "Field {field} not found in Object Type {object}")
            }
            QueryBuilderError::InputObjectFieldNotFound {
                input_object,
                field,
            } => {
                write!(
                    f,
                    "Field {field} not found in Input Object Type {input_object}"
                )
            }
            QueryBuilderError::ArgumentNotFound {
                object,
                field,
                argument,
            } => write!(
                f,
                "Argument {argument} for field {field} not found in Object Type {object}"
            ),
            QueryBuilderError::Unexpected(s) => write!(f, "Unexpected: {s}"),
            QueryBuilderError::QueryFieldNotFound { field } => {
                write!(f, "Field {field} not found in Query type")
            }
            QueryBuilderError::MutationFieldNotFound { field } => {
                write!(f, "Field {field} not found in Mutation type")
            }
            QueryBuilderError::MisshapenHeadersArgument(headers) => {
                write!(f, "Misshapen headers argument: {}", headers)
            }
            QueryBuilderError::MissingVariable(name) => write!(f, "Missing variable {name}"),
        }
    }
}
