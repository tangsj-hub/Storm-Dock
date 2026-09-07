import type { ModelFormat, ModelSource, RemoteModelHit } from "../../lib/types";
import { useHfModelBrowse, type BrowseState } from "./useHfModelBrowse";
import { useMsModelBrowse } from "./useMsModelBrowse";

const idle: BrowseState = {
  hits: [],
  isLoading: false,
  isLoadingMore: false,
  hasMore: false,
  fetchMore: () => undefined,
  reload: () => undefined,
};

export function useRemoteModelBrowse(
  source: ModelSource,
  query: string,
  format: ModelFormat,
  enabled: boolean,
): BrowseState {
  const hf = useHfModelBrowse(query, format, enabled && source === "huggingface");
  const ms = useMsModelBrowse(query, format, enabled && source === "modelscope");
  if (!enabled) return idle;
  return source === "huggingface" ? hf : ms;
}

export type { BrowseState, RemoteModelHit };
