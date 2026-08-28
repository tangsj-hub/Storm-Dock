import type { ToolName } from "../../lib/tools";
import claude from "../../assets/tools/claude.svg";
import openai from "../../assets/tools/openai.svg";
import gemini from "../../assets/tools/gemini.svg";
import grok from "../../assets/tools/grok.svg";
import opencode from "../../assets/tools/opencode-logo-light.svg";
import claw from "../../assets/tools/claw.svg";
import hermes from "../../assets/tools/hermes.png";
import pi from "../../assets/tools/pi.svg";

export const TOOL_ICONS: Record<ToolName, string> = {
  claude,
  codex: openai,
  gemini,
  grok,
  opencode,
  openclaw: claw,
  hermes,
  pi
};
