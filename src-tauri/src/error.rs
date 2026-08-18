use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum AppError {
    #[error("{0}")]
    Message(String),
    #[error("account not found")]
    AccountNotFound,
    #[error("账户凭证缺失，请重新导入该账户")]
    SecretMissing,
    #[error("Token is empty or too short")]
    InvalidToken,
    #[error("JSON does not contain a supported Cursor session")]
    InvalidImport,
    #[error("this application is not supported yet")]
    ComingSoon,
    #[error("登录已取消")]
    LoginCancelled,
    #[error("Cursor 官方登录超时，请重试")]
    LoginTimeout,
    #[error("unsupported Cursor data: {0}")]
    UnsupportedCursor(String),
    #[error("Cursor is not installed or has not been started")]
    CursorNotDetected,
    #[error("could not verify the Cursor session; the previous state was restored")]
    VerifyFailed,
    #[error("could not restore the previous Cursor session")]
    RestoreFailed,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
}

pub(crate) type Result<T> = std::result::Result<T, AppError>;
