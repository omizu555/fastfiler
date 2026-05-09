use serde::Serialize;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum AppError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid path: {0}")]
    InvalidPath(String),
    #[error("not supported: {0}")]
    NotSupported(String),
    #[error("watch error: {0}")]
    Watch(String),
    #[error("canceled")]
    Canceled,
    #[error("plugin error: {0}")]
    Plugin(String),
    #[error("win32 error: {0}")]
    Win32(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("env var missing: {0}")]
    EnvMissing(&'static str),
    #[error("other: {0}")]
    Other(String),
}

impl AppError {
    /// 機械可読な分類タグ。フロント側で error.kind を見たい場合に使用。
    pub fn kind(&self) -> &'static str {
        match self {
            AppError::Io(_) => "io",
            AppError::NotFound(_) => "not_found",
            AppError::InvalidPath(_) => "invalid_path",
            AppError::NotSupported(_) => "not_supported",
            AppError::Watch(_) => "watch",
            AppError::Canceled => "canceled",
            AppError::Plugin(_) => "plugin",
            AppError::Win32(_) => "win32",
            AppError::Parse(_) => "parse",
            AppError::EnvMissing(_) => "env_missing",
            AppError::Other(_) => "other",
        }
    }
}

// 既存フロント (Tauri invoke().catch(e => string)) との互換のため、
// シリアライズはこれまで通り Display 文字列のみを返す。
impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
