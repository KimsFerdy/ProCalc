/**
 * Curve colours: a validated categorical palette (colour-blind safe on adjacent slots,
 * checked against ProCalc's panel colours in both themes). Slots are handed out in this
 * fixed order and stay with their line, so a curve never changes colour when others come
 * and go. Light-mode slots 3–5 are under 3:1 on white, so every curve is also labelled.
 */
const LIGHT = ["#2a78d6", "#eb6834", "#1baf7a", "#eda100", "#e87ba4", "#008300", "#6250d6", "#e34948"];
const DARK = ["#3987e5", "#d95926", "#199e70", "#c98500", "#d55181", "#008300", "#9085e9", "#e66767"];

export const SLOTS = LIGHT.length;

export function prefersDark(): boolean {
  return typeof matchMedia !== "undefined" && matchMedia("(prefers-color-scheme: dark)").matches;
}

export function slotColor(slot: number, dark = prefersDark()): string {
  return (dark ? DARK : LIGHT)[slot % SLOTS];
}
