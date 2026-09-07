import type { ModelFormat, ModelSource, RemoteModelHit } from "../../lib/types";
import { IDLE_BROWSE_STATE, type BrowseState } from "./modelBrowseShared";
import { useHfModelBrowse } from "./useHfModelBrowse";
import { useMsModelBrowse } from "./useMsModelBrowse";

export function useRemoteModelBrowse(
  source: ModelSource,
  query: string,
  format: ModelFormat,
  enabled: boolean,
): BrowseState {
  const hf = useHfModelBrowse(query, format, enabled && source === "huggingface");
  const ms = useMsModelBrowse(query, format, enabled && source === "modelscope");
  if (!enabled) return IDLE_BROWSE_STATE;
  return source === "huggingface" ? hf : ms;
}

export type { BrowseState, RemoteModelHit };
