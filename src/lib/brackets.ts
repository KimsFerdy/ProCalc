// Colour-coded brackets: each pair takes the colour of its nesting level (--br-0 … --br-5 in
// app.css), so it's easy to see which ( closes which ). Works on MathLive's markup, both the
// rendered answers and the live input field.

const LEVELS = 6;
const OPENERS = new Set(["(", "[", "{", "⟨", "⌈", "⌊"]);
const CLOSERS = new Set([")", "]", "}", "⟩", "⌉", "⌋"]);
const FENCE = ".ML__open, .ML__close";

/** Colours every bracket under `root` by its depth. Safe to run again on the same markup. */
export function colorBrackets(root: ParentNode) {
  let depth = 0;
  for (const el of root.querySelectorAll<HTMLElement>("span")) {
    if (el.classList.contains("ML__nulldelimiter")) continue;
    let opens: boolean;
    if (el.classList.contains("ML__open")) opens = true;
    else if (el.classList.contains("ML__close")) opens = false;
    else if (el.childElementCount === 0) {
      const t = el.textContent?.trim() ?? "";
      if (OPENERS.has(t)) opens = true;
      else if (CLOSERS.has(t)) opens = false;
      else continue;
      // A glyph inside a tall bracket already coloured as a whole.
      if (el.parentElement?.closest(FENCE)) continue;
    } else continue;

    if (opens) {
      el.style.color = `var(--br-${depth % LEVELS})`;
      depth++;
    } else {
      depth = Math.max(0, depth - 1);
      el.style.color = `var(--br-${depth % LEVELS})`;
    }
  }
}

/** Rendered markup (an HTML string) with its brackets coloured. */
export function colorBracketsInMarkup(markup: string): string {
  if (typeof document === "undefined" || !/[([{⟨⌈⌊]/.test(markup)) return markup;
  const t = document.createElement("template");
  t.innerHTML = markup;
  colorBrackets(t.content);
  return t.innerHTML;
}

/** Keeps a MathLive field's brackets coloured as it re-renders while you type. */
export function watchFieldBrackets(field: HTMLElement): () => void {
  const root = field.shadowRoot;
  if (!root) return () => {};
  let frame = 0;
  const recolor = () => {
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(() => colorBrackets(root));
  };
  // Only structural changes: the colours themselves are attribute changes and don't loop.
  const mo = new MutationObserver(recolor);
  mo.observe(root, { childList: true, subtree: true, characterData: true });
  recolor();
  return () => {
    cancelAnimationFrame(frame);
    mo.disconnect();
  };
}
