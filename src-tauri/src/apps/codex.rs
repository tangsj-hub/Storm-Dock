use crate::apps::ApplicationAdapter;
use crate::error::{AppError, Result};
use crate::models::{ApplicationKind, ApplicationStatus, Session};

pub(crate) struct CodexAdapter;

impl ApplicationAdapter for CodexAdapter {
    fn kind(&self) -> ApplicationKind {
        ApplicationKind::Codex
    }

    fn detect(&self) -> ApplicationStatus {
        ApplicationStatus {
            kind: self.kind(),
            label: self.kind().display_name().into(),
            available: false,
            reason: Some("桌面端账号切换待支持".into()),
        }
    }

    fn import_current(&self) -> Result<Session> {
        Err(AppError::ComingSoon)
    }

    fn apply(&self, _: &Session) -> Result<()> {
        Err(AppError::ComingSoon)
    }

    fn is_running(&self) -> bool {
        false
    }
}
