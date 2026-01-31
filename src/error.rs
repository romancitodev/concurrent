use thiserror::Error;

#[derive(Debug, Clone)]
pub struct ValidationError {
    pub kind: ValidationErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValidationErrorKind {
    CircularDependency,
    MissingDependency,
}

impl ValidationError {
    pub fn new(kind: ValidationErrorKind, message: String) -> Self {
        Self { kind, message }
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Errors: {0:#?}")]
    InvalidGraph(Vec<ValidationError>),

    #[error("Invalid parameters")]
    InvalidParams,

    #[error("Invalid type: {0}")]
    InvalidType(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Render error: {0}")]
    RenderError(String),
}

impl From<Vec<ValidationError>> for Error {
    fn from(errors: Vec<ValidationError>) -> Self {
        Error::InvalidGraph(errors)
    }
}
