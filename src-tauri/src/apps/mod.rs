mod codex;
mod cursor;
mod grok;
mod workbuddy;

pub(crate) use codex::CodexAdapter;
pub(crate) use cursor::CursorAdapter;
pub(crate) use grok::GrokAdapter;
pub(crate) use workbuddy::WorkbuddyAdapter;

use crate::error::Result;
use crate::models::{ApplicationKind, ApplicationStatus, Session};

pub(crate) trait ApplicationAdapter {
    fn kind(&self) -> ApplicationKind;
    fn detect(&self) -> ApplicationStatus;
    fn import_current(&self) -> Result<Session>;
    fn apply(&self, session: &Session) -> Result<()>;
}
