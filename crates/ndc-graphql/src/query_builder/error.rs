use http::StatusCode;
use ndc_sdk::{
    connector::ErrorResponse,
    models::{ArgumentName, CollectionName, FieldName, ProcedureName, TypeName, VariableName},
};

#[derive(Debug, thiserror::Error)]
pub enum QueryBuilderError {
    #[error("Object Type {0} not found in configuration")]
    ObjectTypeNotFound(TypeName),
    #[error("Input Object Type {0} not found in configuration")]
    InputObjectTypeNotFound(TypeName),
    #[error("No fields for query")]
    NoRequesQueryFields,
    #[error("No query type in schema definition")]
    NoQueryType,
    #[error("No mutation type in schema definition")]
    NoMutationType,
    #[error("Feature not supported: {0}")]
    NotSupported(String),
    #[error("Field {field} not found in Query type")]
    QueryFieldNotFound { field: CollectionName },
    #[error("Field {field} not found in Mutation type")]
    MutationFieldNotFound { field: ProcedureName },
    #[error("Field {field} not found in Object Type {object}")]
    ObjectFieldNotFound { object: TypeName, field: FieldName },
    #[error("Field {field} not found in Input Object Type {input_object}")]
    InputObjectFieldNotFound {
        input_object: TypeName,
        field: FieldName,
    },
    #[error("Argument {argument} for field {field} not found in Object Type {object}")]
    ArgumentNotFound {
        object: TypeName,
        field: FieldName,
        argument: ArgumentName,
    },
    #[error("Misshapen headers argument: {0}")]
    MisshapenHeadersArgument(serde_json::Value),
    #[error("Unexpected: {0}")]
    Unexpected(String),
    #[error("Missing variable {0}")]
    MissingVariable(VariableName),
}

impl From<QueryBuilderError> for ErrorResponse {
    fn from(value: QueryBuilderError) -> Self {
        let status = match value {
            QueryBuilderError::ObjectTypeNotFound(_)
            | QueryBuilderError::InputObjectTypeNotFound(_) => StatusCode::INTERNAL_SERVER_ERROR,
            QueryBuilderError::NoRequesQueryFields
            | QueryBuilderError::NoQueryType
            | QueryBuilderError::NoMutationType
            | QueryBuilderError::NotSupported(_)
            | QueryBuilderError::QueryFieldNotFound { .. }
            | QueryBuilderError::MutationFieldNotFound { .. }
            | QueryBuilderError::ObjectFieldNotFound { .. }
            | QueryBuilderError::InputObjectFieldNotFound { .. }
            | QueryBuilderError::ArgumentNotFound { .. }
            | QueryBuilderError::MisshapenHeadersArgument(_)
            | QueryBuilderError::Unexpected(_)
            | QueryBuilderError::MissingVariable(_) => StatusCode::BAD_REQUEST,
        };
        ErrorResponse::new(status, value.to_string(), serde_json::Value::Null)
    }
}
