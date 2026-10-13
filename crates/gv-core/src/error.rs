use thiserror::Error;

/// Unified error type for `graphvis-rs` replacing C Graphviz's `graphviz_exit()` and `agerr()`.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum GraphvizError {
    #[error("DOT syntax error at line {line}, column {col}: {message}")]
    SyntaxError {
        line: usize,
        col: usize,
        message: String,
    },

    #[error("Unsupported layout engine: '{0}'")]
    UnsupportedEngine(String),

    #[error("Unsupported output format: '{0}'")]
    UnsupportedFormat(String),

    #[error("Layout computation error in {stage}: {message}")]
    LayoutError {
        stage: &'static str,
        message: String,
    },

    #[error("Invalid attribute '{attr}': {message}")]
    AttributeError { attr: String, message: String },
}

pub type Result<T> = std::result::Result<T, GraphvizError>;
