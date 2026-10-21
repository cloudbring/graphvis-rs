use thiserror::Error;

/// Unified error type for `graphviz-pure` replacing C Graphviz's `graphviz_exit()` and `agerr()`.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum GraphvizError {
    /// A DOT syntax error. `message` is worded as cgraph's `aagerror` (`lib/cgraph/scan.l`),
    /// e.g. `syntax error in line 3 near ';'`, and `line` is the line it reports (cgraph's
    /// `int line_num`).
    #[error("{message}")]
    SyntaxError {
        line: i32,
        col: usize,
        message: String,
        /// The file name set by a `# N "file"` line directive, which C reports instead of
        /// the input's name.
        file: Option<String>,
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

    #[error("Rendering error: {0}")]
    RenderError(String),
}

pub type Result<T> = std::result::Result<T, GraphvizError>;
