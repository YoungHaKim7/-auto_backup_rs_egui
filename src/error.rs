use thiserror::Error;

#[derive(Error, Debug)]
pub enum BackupError {
    #[error("File I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Failed to parse data: {0}")]
    Parse(String),

    #[error("Path conversion error: {0}")]
    Path(String),

    #[error("Execution error: {0}")]
    Execute(String),
}
