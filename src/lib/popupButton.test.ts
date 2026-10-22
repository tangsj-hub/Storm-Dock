import { describe, expect, it } from "vitest";
import { consumePopupButtonPress } from "./popupButton";

describe("popup button press", () => {
  it("runs on the primary pointer press, which is all a non-activating Windows webview delivers", () => {
    expect(consumePopupButtonPress({ type: "pointerdown", button: 0 }, false)).toEqual({ run: true, armed: true });
  });

  it("does not run the click that follows that press", () => {
    const pressed = consumePopupButtonPress({ type: "pointerdown", button: 0 }, false);
    expect(consumePopupButtonPress({ type: "click" }, pressed.armed)).toEqual({ run: false, armed: false });
  });

  it("still runs a keyboard click that had no pointer press", () => {
    expect(consumePopupButtonPress({ type: "click" }, false)).toEqual({ run: true, armed: false });
  });

  it("ignores a non-primary pointer press", () => {
    expect(consumePopupButtonPress({ type: "pointerdown", button: 2 }, false)).toEqual({ run: false, armed: false });
  });
});
