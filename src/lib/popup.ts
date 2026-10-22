import { invoke } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useEffect, useState } from "react";

/** What the backend popup manager pushes to a persistent popup window on each show. */
export type PopupEnvelope<T> = { generation: number; payload: T };

/** Pointer over the popup pauses its auto-hide (macOS banner behaviour). */
export const setPopupHover = (generation: number, hovering: boolean) => invoke<void>("popup_hover", { generation, hovering }).catch(() => undefined);

/** Subscribe once per window lifetime; pulls the latest payload on mount. Newer generation wins. */
export function usePopupPayload<T>() {
  const [envelope, setEnvelope] = useState<PopupEnvelope<T> | null>(null);
  useEffect(() => {
    let latest = 0;
    const accept = (next: PopupEnvelope<T> | null) => {
      if (!next || next.generation <= latest) return;
      latest = next.generation;
      setEnvelope(next);
    };
    const unlisten = getCurrentWebviewWindow().listen<PopupEnvelope<T>>("popup-payload", (event) => accept(event.payload));
    void invoke<PopupEnvelope<T> | null>("get_popup_payload").then(accept).catch(() => undefined);
    return () => { void unlisten.then((stop) => stop()); };
  }, []);
  return envelope;
}
