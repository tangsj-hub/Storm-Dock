import type { RemoteModelHit } from "../../lib/types";
import { HF_FETCH_TIMEOUT_MESSAGE, isAbortError } from "../../lib/fetchTimeout";

export type BrowseState = {
  hits: RemoteModelHit[];
  isLoading: boolean;
  isLoadingMore: boolean;
  hasMore: boolean;
  error?: string;
  fetchMore: () => void;
  reload: () => void;
};

export const IDLE_BROWSE_STATE: BrowseState = {
  hits: [],
  isLoading: false,
  isLoadingMore: false,
  hasMore: false,
  fetchMore: () => undefined,
  reload: () => undefined,
};

/** Debounce for query/format changes before kicking a browse request. */
export const BROWSE_DEBOUNCE_MS = 350;

export { isAbortError };

/** True for timeout / TypeError / common browser network failure strings. */
export function isLikelyNetworkError(error: unknown): boolean {
  if (error instanceof Error && error.message === HF_FETCH_TIMEOUT_MESSAGE) return true;
  if (error instanceof TypeError) return true;
  const message = error instanceof Error ? error.message : String(error);
  return /failed to fetch|networkerror|load failed|network request failed|fetch failed/i.test(message);
}

/**
 * Fail-soft browse error text: AbortError → undefined (caller ignores);
 * optional unreachable label for network/timeout; otherwise message string.
 */
export function browseFailSoftMessage(
  error: unknown,
  unreachableLabel?: string,
): string | undefined {
  if (isAbortError(error)) return;
  if (unreachableLabel && isLikelyNetworkError(error)) return unreachableLabel;
  return error instanceof Error ? error.message : String(error);
}

/**
 * Race a promise against an AbortSignal so UI can cancel waiting even when the
 * underlying call (e.g. Tauri invoke) cannot be aborted mid-flight.
 */
export function raceAbort<T>(promise: Promise<T>, signal?: AbortSignal): Promise<T> {
  if (!signal) return promise;
  if (signal.aborted) {
    return Promise.reject(new DOMException("Aborted", "AbortError"));
  }
  return new Promise<T>((resolve, reject) => {
    const onAbort = () => {
      reject(new DOMException("Aborted", "AbortError"));
    };
    signal.addEventListener("abort", onAbort, { once: true });
    promise.then(
      (value) => {
        signal.removeEventListener("abort", onAbort);
        resolve(value);
      },
      (error) => {
        signal.removeEventListener("abort", onAbort);
        reject(error);
      },
    );
  });
}
