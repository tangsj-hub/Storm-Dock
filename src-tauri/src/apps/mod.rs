mod codex;
mod cursor;

pub(crate) use codex::CodexAdapter;
pub(crate) use cursor::{
    launch_cursor, terminate_cursor, wait_for_cursor_stop, CursorAdapter,
};

use crate::error::Result;
use crate::models::{ApplicationKind, ApplicationStatus, Session};

pub(crate) trait ApplicationAdapter {
    fn kind(&self) -> ApplicationKind;
    fn detect(&self) -> ApplicationStatus;
    fn import_current(&self) -> Result<Session>;
    fn apply(&self, session: &Session) -> Result<()>;
    fn is_running(&self) -> bool;
}
