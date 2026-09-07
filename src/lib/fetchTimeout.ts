/** Stable error message key for HF list timeouts / unreachable Hub. UI maps via i18n `modelHfUnreachable`. */
export const HF_FETCH_TIMEOUT_MESSAGE = "modelHfUnreachable";

/** Default bound for Hugging Face listModels HTTP calls (fail fast on bad networks). */
export const HF_FETCH_TIMEOUT_MS = 7000;

export type FetchWithTimeoutOptions = RequestInit & {
  timeoutMs?: number;
  /** Underlying fetch implementation (defaults to global fetch). */
  fetch?: typeof fetch;
};

export function isAbortError(error: unknown): boolean {
  if (error instanceof DOMException && error.name === "AbortError") return true;
  if (error instanceof Error && error.name === "AbortError") return true;
  return false;
}

function abortedError(signal?: AbortSignal): Error {
  const reason = signal?.reason;
  if (reason instanceof Error) return reason;
  return new DOMException("Aborted", "AbortError");
}

/** Compose multiple AbortSignals; aborts when any input aborts. */
export function composeAbortSignals(a: AbortSignal, b?: AbortSignal): AbortSignal {
  if (!b) return a;
  const anyFn = (AbortSignal as typeof AbortSignal & { any?: (signals: AbortSignal[]) => AbortSignal }).any;
  if (typeof anyFn === "function") return anyFn([a, b]);
  if (a.aborted || b.aborted) {
    const controller = new AbortController();
    controller.abort();
    return controller.signal;
  }
  const controller = new AbortController();
  const onAbort = () => controller.abort();
  a.addEventListener("abort", onAbort, { once: true });
  b.addEventListener("abort", onAbort, { once: true });
  return controller.signal;
}

/**
 * fetch with a hard timeout. Composes an optional user AbortSignal with the timeout.
 * - User abort → AbortError (DOMException)
 * - Timeout → Error with message {@link HF_FETCH_TIMEOUT_MESSAGE}
 */
export async function fetchWithTimeout(
  input: RequestInfo | URL,
  init?: FetchWithTimeoutOptions,
): Promise<Response> {
  const timeoutMs = init?.timeoutMs ?? HF_FETCH_TIMEOUT_MS;
  const baseFetch = init?.fetch ?? fetch;
  const userSignal = init?.signal;
  const { timeoutMs: _t, fetch: _f, signal: _s, ...rest } = init ?? {};

  if (userSignal?.aborted) {
    throw abortedError(userSignal);
  }

  const anyFn = (AbortSignal as typeof AbortSignal & { any?: (signals: AbortSignal[]) => AbortSignal }).any;
  const timeoutFn = (AbortSignal as typeof AbortSignal & { timeout?: (ms: number) => AbortSignal }).timeout;

  if (typeof anyFn === "function" && typeof timeoutFn === "function") {
    const timeoutSignal = timeoutFn(timeoutMs);
    const signal = userSignal ? anyFn([userSignal, timeoutSignal]) : timeoutSignal;
    try {
      return await baseFetch(input, { ...rest, signal });
    } catch (error) {
      if (userSignal?.aborted) throw abortedError(userSignal);
      if (timeoutSignal.aborted) {
        throw new Error(HF_FETCH_TIMEOUT_MESSAGE);
      }
      if (isAbortError(error) && timeoutSignal.aborted) {
        throw new Error(HF_FETCH_TIMEOUT_MESSAGE);
      }
      throw error;
    }
  }

  const controller = new AbortController();
  let timedOut = false;
  const timer = setTimeout(() => {
    timedOut = true;
    controller.abort();
  }, timeoutMs);

  const onUserAbort = () => {
    controller.abort();
  };
  userSignal?.addEventListener("abort", onUserAbort, { once: true });

  try {
    return await baseFetch(input, { ...rest, signal: controller.signal });
  } catch (error) {
    if (userSignal?.aborted) throw abortedError(userSignal);
    if (timedOut) throw new Error(HF_FETCH_TIMEOUT_MESSAGE);
    throw error;
  } finally {
    clearTimeout(timer);
    userSignal?.removeEventListener("abort", onUserAbort);
  }
}
