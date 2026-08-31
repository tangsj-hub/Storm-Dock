use super::adapter::ToolAdapter;

pub(crate) const ADAPTER: GeminiAdapter = GeminiAdapter;

pub(crate) struct GeminiAdapter;

impl ToolAdapter for GeminiAdapter {
    fn name(&self) -> &'static str {
        "gemini"
    }
    fn display_name(&self) -> &'static str {
        "Gemini CLI"
    }
    fn npm_package(&self) -> Option<&'static str> {
        Some("@google/gemini-cli")
    }
}
