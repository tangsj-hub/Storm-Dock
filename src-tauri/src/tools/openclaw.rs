use super::adapter::ToolAdapter;

pub(crate) const ADAPTER: OpenClawAdapter = OpenClawAdapter;

pub(crate) struct OpenClawAdapter;

impl ToolAdapter for OpenClawAdapter {
    fn name(&self) -> &'static str {
        "openclaw"
    }
    fn display_name(&self) -> &'static str {
        "OpenClaw"
    }
    fn npm_package(&self) -> Option<&'static str> {
        Some("openclaw")
    }
}
