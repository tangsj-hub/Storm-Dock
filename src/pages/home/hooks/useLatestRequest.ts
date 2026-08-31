import { useCallback, useEffect, useRef } from "react";

/** Allows only the newest asynchronous result to update the owning view. */
export function useLatestRequest() {
  const generation = useRef(0);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => { mounted.current = false; };
  }, []);
  return useCallback(() => {
    const request = ++generation.current;
    return () => mounted.current && request === generation.current;
  }, []);
}
