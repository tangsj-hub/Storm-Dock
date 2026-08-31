use super::adapter::ToolAdapter;

pub(crate) const ADAPTER: PiAdapter = PiAdapter;

pub(crate) struct PiAdapter;

impl ToolAdapter for PiAdapter {
    fn name(&self) -> &'static str {
        "pi"
    }
    fn display_name(&self) -> &'static str {
        "Pi"
    }
    fn npm_package(&self) -> Option<&'static str> {
        Some("@earendil-works/pi-coding-agent")
    }
}
