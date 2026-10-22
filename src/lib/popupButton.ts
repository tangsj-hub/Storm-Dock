/** How a popup button should treat one DOM event.
 *
 * A non-activating Windows webview delivers `pointerdown` and often never
 * synthesizes `click`. A normal mouse click does both, so the follow-up click
 * must not run the action a second time. Keyboard activation is a `click`
 * with no preceding pointer press.
 */
export function consumePopupButtonPress(
  event: { type: "pointerdown" | "click"; button?: number },
  armed: boolean,
): { run: boolean; armed: boolean } {
  if (event.type === "pointerdown") {
    if ((event.button ?? 0) !== 0) return { run: false, armed };
    return { run: true, armed: true };
  }
  if (armed) return { run: false, armed: false };
  return { run: true, armed: false };
}
