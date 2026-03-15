/// OpenJPEG error types.
#[derive(Debug)]
pub enum Error {
    /// Invalid input parameter or data.
    InvalidInput(String),
    /// Output buffer is too small.
    BufferTooSmall,
    /// Unexpected end of stream.
    EndOfStream,
    /// I/O error.
    IoError(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
            Error::BufferTooSmall => write!(f, "buffer too small"),
            Error::EndOfStream => write!(f, "end of stream"),
            Error::IoError(msg) => write!(f, "I/O error: {msg}"),
        }
    }
}

impl std::error::Error for Error {}

/// Result type alias using [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display() {
        let err = Error::InvalidInput("test".to_string());
        let msg = format!("{err}");
        assert!(msg.contains("test"));
    }

    #[test]
    fn error_std_error() {
        let err = Error::InvalidInput("test".to_string());
        let _: &dyn std::error::Error = &err;
    }

    #[test]
    fn error_variants() {
        let _ = Error::InvalidInput("msg".to_string());
        let _ = Error::BufferTooSmall;
        let _ = Error::EndOfStream;
        let _ = Error::IoError("io".to_string());
    }

    #[test]
    fn result_type_alias() {
        let ok: Result<i32> = Ok(42);
        assert!(ok.is_ok());

        let err: Result<i32> = Err(Error::BufferTooSmall);
        assert!(err.is_err());
    }
}
