use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum AppError {
    #[error("{0}")]
    Message(String),
    #[error("account not found")]
    AccountNotFound,
    #[error("账户凭证缺失，请重新导入该账户")]
    SecretMissing,
    #[error("账号已被封禁")]
    AccountBlocked,
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
    #[error("未检测到 ChatGPT 登录")]
    CodexNotDetected,
    #[error("未检测到 Grok 登录")]
    GrokNotDetected,
    #[error("未检测到 WorkBuddy 登录")]
    WorkbuddyNotDetected,
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
    #[error(transparent)]
    Http(#[from] crate::http::HttpError),
}

pub(crate) type Result<T> = std::result::Result<T, AppError>;

pub(crate) fn is_account_blocked_message(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("user account is blocked")
        || lower.contains("account is blocked")
        || message.contains("账号已被封禁")
        || message.contains("账号已封禁")
}

pub(crate) fn is_token_invalid_message(message: &str) -> bool {
    !is_account_blocked_message(message)
        && (message.contains("失效") || message.contains("过期") || lower_contains_expired(message))
}

fn lower_contains_expired(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("invalid_grant") || lower.contains("token expired") || lower.contains("expired")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_blocked_account_messages() {
        assert!(is_account_blocked_message("User account is blocked"));
        assert!(is_account_blocked_message("账号已被封禁"));
        assert!(!is_account_blocked_message("Grok 登录已失效，请重新官方登录。"));
        assert!(is_token_invalid_message("Grok 登录已失效，请重新官方登录。"));
        assert!(!is_token_invalid_message("User account is blocked"));
    }
}
