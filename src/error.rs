/// OpenJPEG error types.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Invalid input parameter or data.
    #[error("invalid input: {0}")]
    InvalidInput(String),
    /// Output buffer is too small.
    #[error("buffer too small")]
    BufferTooSmall,
    /// Unexpected end of stream.
    #[error("end of stream")]
    EndOfStream,
    /// I/O error.
    #[error("I/O error: {0}")]
    IoError(String),
}

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
