<script lang="ts">
  import { onMount, tick } from "svelte";
  import {
    evaluateSheet,
    plotSheet,
    type Answer,
    type Curve,
    type Outcome,
    type Settings,
    type View as PlotView,
  } from "$lib/engine";
  import Graph, { type Series } from "$lib/Graph.svelte";
  import type { PaperState } from "$lib/project";
  import { SLOTS, slotColor } from "$lib/palette";
  import { watchFieldBrackets } from "$lib/brackets";
  import { workedRows } from "$lib/worked";

  interface Line {
    id: number;
    kind: "math" | "text";
    /** LaTeX for maths lines, plain text for notes. */
    src: string;
  }

  type MathField = HTMLElement & {
    value: string;
    focus(): void;
    executeCommand(c: unknown): boolean;
    position: number;
    lastOffset: number;
    inlineShortcuts: Record<string, unknown>;
    keybindings: readonly unknown[];
  };

  let {
    settings,
    render,
    onpin,
    onchange,
  }: {
    settings: Settings;
    render: (latex: string) => string;
    /** Ctrl+Enter or the pin button: copy a line and its answer to the history. */
    onpin: (latex: string, answer: Answer) => void;
    /** Anything worth saving changed (lines, the graph, the view). */
    onchange?: () => void;
  } = $props();

  const STORE_KEY = "procalc.paper.v1";
  /**
   * Typing one of these turns it into a unit. Single letters (m, s, g) stay variables, as do
   * names that read as products in physics (mg is m·g) or clash with MathLive (psi is ψ);
   * write those as \mathrm{m}, or after → where every letter spells a unit.
   */
  const UNIT_SHORTCUTS = [
    "km", "cm", "mm", "kg", "ms", "min", "Hz", "kHz", "MHz", "GHz", "Pa", "kPa", "MPa", "kN",
    "kJ", "MJ", "kW", "MW", "Wh", "kWh", "eV", "keV", "MeV", "mol", "mph", "ft", "yd", "lb",
    "oz", "gal", "atm", "bar", "cal", "kcal", "mL",
  ];

  let lines = $state<Line[]>([{ id: 1, kind: "math", src: "" }]);
  let results = $state<Record<number, Outcome>>({});
  /** Answer view or Steps view (worked solutions under each line). */
  let showSteps = $state(false);
  /** The method picked for each line's working, by line id. */
  let chosenMethod = $state<Record<number, number>>({});
  /** The graph panel above the lines. */
  let showGraph = $state(false);
  /** Share of the Paper's height the graph takes, in percent (the divider moves it). */
  let graphShare = $state(58);
  let view = $state({ xMin: -10, xMax: 10, yMin: -6, yMax: 6 });
  /** What each line draws, by line id, and the rectangle it was sampled over. */
  let curves = $state<Record<number, Curve>>({});
  let curveRect = $state({ xMin: -10, xMax: 10, yMin: -6, yMax: 6 });
  /** The graph's size on screen, for how finely to sample. */
  let plotSize = { width: 600, height: 260 };
  let plotSeq = 0;
  /** Each plotted line keeps its colour slot for as long as it exists. */
  let slots = $state<Record<number, number>>({});
  let activeId = $state<number | null>(null);
  /** Height of MathLive's on-screen keyboard, so the lines stay above it. */
  let keyboardHeight = $state(0);
  let nextId = 2;
  let evalTimer: ReturnType<typeof setTimeout> | undefined;
  let evalSeq = 0;
  const fields = new Map<number, MathField>();
  const notes = new Map<number, HTMLTextAreaElement>();

  function load() {
    try {
      const saved = JSON.parse(localStorage.getItem(STORE_KEY) ?? "null");
      if (Array.isArray(saved?.lines) && saved.lines.length) {
        lines = saved.lines.filter((l: Line) => typeof l?.src === "string");
        nextId = Math.max(0, ...lines.map((l) => l.id)) + 1;
      }
      if (typeof saved?.showSteps === "boolean") showSteps = saved.showSteps;
      if (typeof saved?.showGraph === "boolean") showGraph = saved.showGraph;
      if (typeof saved?.graphShare === "number") graphShare = saved.graphShare;
      if (saved?.view && ["xMin", "xMax", "yMin", "yMax"].every((k) => Number.isFinite(saved.view[k]))) view = saved.view;
    } catch {
      /* first run, or storage unavailable */
    }
    if (!lines.length) lines = [{ id: nextId++, kind: "math", src: "" }];
  }

  function save() {
    try {
      localStorage.setItem(STORE_KEY, JSON.stringify({ lines, showSteps, showGraph, view, graphShare }));
    } catch {
      /* storage unavailable: the Paper just won't persist */
    }
    onchange?.();
  }

  /** Everything a project file keeps of this Paper. */
  export function getState(): PaperState {
    return {
      lines: lines.map((l) => ({ kind: l.kind, src: l.src })),
      showSteps,
      showGraph,
      graphShare,
      view: { ...view },
    };
  }

  /** Replaces the Paper with a saved one (or a blank one for New). */
  export function setState(s: PaperState) {
    // Fresh ids, so no old line's field or answer is reused.
    lines = s.lines.map((l, i) => ({ id: nextId + i, kind: l.kind, src: l.src }));
    nextId += lines.length;
    showSteps = s.showSteps;
    showGraph = s.showGraph;
    graphShare = s.graphShare;
    view = { ...s.view };
    results = {};
    curves = {};
    slots = {};
    chosenMethod = {};
    save();
    scheduleEval(0);
  }

  onMount(() => {
    load();
    scheduleEval(0);
    const kb = (window as any).mathVirtualKeyboard;
    const onGeometry = () => (keyboardHeight = kb?.visible ? kb.boundingRect.height : 0);
    kb?.addEventListener("geometrychange", onGeometry);
    return () => kb?.removeEventListener("geometrychange", onGeometry);
  });

  // Settings (digits, DEG/RAD) change every answer.
  $effect(() => {
    void settings.digits;
    void settings.angle;
    scheduleEval(0);
  });

  function scheduleEval(delay = 120) {
    clearTimeout(evalTimer);
    evalTimer = setTimeout(runEval, delay);
  }

  async function runEval() {
    const math = lines.filter((l) => l.kind === "math" && l.src.trim());
    const seq = ++evalSeq;
    const out = math.length ? await evaluateSheet(math.map((l) => l.src), $state.snapshot(settings)) : [];
    if (seq !== evalSeq) return;
    const next: Record<number, Outcome> = {};
    math.forEach((l, k) => (next[l.id] = out[k]));
    results = next;
    if (showGraph) refreshPlot();
  }

  // ---------- Graph ----------

  function onRange(v: PlotView) {
    plotSize = { width: Math.max(50, Math.min(2000, v.width)), height: Math.max(50, Math.min(2000, v.height)) };
    refreshPlot({ xMin: v.xMin, xMax: v.xMax, yMin: v.yMin, yMax: v.yMax });
  }

  /** Samples what the lines draw over a rectangle (by default the current view). */
  async function refreshPlot(rect = { xMin: view.xMin, xMax: view.xMax, yMin: view.yMin, yMax: view.yMax }) {
    const math = lines.filter((l) => l.kind === "math" && l.src.trim());
    const seq = ++plotSeq;
    // 16 digits is far finer than any screen, and much quicker.
    const fast = { ...$state.snapshot(settings), digits: 16 };
    const out = math.length ? await plotSheet(math.map((l) => l.src), fast, { ...rect, ...plotSize }) : [];
    if (seq !== plotSeq) return;
    const next: Record<number, Curve> = {};
    math.forEach((l, k) => {
      const c = out[k];
      if (c) next[l.id] = c;
    });
    // Lowest free colour slot for newly plotted lines; slots of removed lines free up.
    const present = new Set(lines.map((l) => l.id));
    const kept: Record<number, number> = {};
    for (const [id, s] of Object.entries(slots)) if (present.has(Number(id))) kept[Number(id)] = s;
    for (const id of Object.keys(next).map(Number)) {
      if (kept[id] === undefined) {
        const used = new Set(Object.values(kept));
        let s = 0;
        while (used.has(s) && s < SLOTS * 4) s++;
        kept[id] = s;
      }
    }
    slots = kept;
    curves = next;
    curveRect = rect;
  }

  function toggleGraph() {
    showGraph = !showGraph;
    if (showGraph) refreshPlot();
    save();
  }

  /** The divider between the graph and the lines. */
  function startSplit(e: PointerEvent) {
    const bar = e.currentTarget as HTMLElement;
    const paper = bar.closest(".paper") as HTMLElement;
    bar.setPointerCapture(e.pointerId);
    const onMove = (m: PointerEvent) => {
      const r = paper.getBoundingClientRect();
      graphShare = Math.max(22, Math.min(82, ((m.clientY - r.top) / r.height) * 100));
    };
    const onUp = () => {
      bar.removeEventListener("pointermove", onMove);
      bar.removeEventListener("pointerup", onUp);
      save();
    };
    bar.addEventListener("pointermove", onMove);
    bar.addEventListener("pointerup", onUp);
  }

  let series = $derived<Series[]>(
    lines.flatMap((l, k) => {
      const c = curves[l.id];
      return c ? [{ id: l.id, slot: slots[l.id] ?? 0, number: k + 1, curve: c }] : [];
    }),
  );

  // Remember where the graph was left.
  $effect(() => {
    void [view.xMin, view.xMax, view.yMin, view.yMax];
    clearTimeout(viewSaveTimer);
    viewSaveTimer = setTimeout(save, 400);
  });
  let viewSaveTimer: ReturnType<typeof setTimeout> | undefined;

  function changed() {
    save();
    scheduleEval();
  }

  // ---------- Lines ----------

  async function focusLine(id: number, atEnd = true) {
    await tick();
    const f = fields.get(id);
    if (f) {
      f.focus();
      if (atEnd) f.position = f.lastOffset;
      return;
    }
    notes.get(id)?.focus();
  }

  function insertAfter(id: number, kind: Line["kind"] = "math") {
    const k = lines.findIndex((l) => l.id === id);
    const line: Line = { id: nextId++, kind, src: "" };
    lines.splice(k + 1, 0, line);
    changed();
    focusLine(line.id);
  }

  function remove(id: number) {
    const k = lines.findIndex((l) => l.id === id);
    if (lines.length === 1) {
      // Keep the same object: the line's field listeners hold on to it.
      lines[0].src = "";
      const f = fields.get(lines[0].id);
      if (f) f.value = "";
    } else {
      lines.splice(k, 1);
      focusLine(lines[Math.max(0, k - 1)].id);
    }
    changed();
  }

  function move(id: number, step: -1 | 1) {
    const k = lines.findIndex((l) => l.id === id) + step;
    if (k >= 0 && k < lines.length) focusLine(lines[k].id, step < 0);
  }

  function toggleKind(line: Line) {
    line.kind = line.kind === "math" ? "text" : "math";
    changed();
    focusLine(line.id);
  }

  function addNote() {
    insertAfter(activeId ?? lines[lines.length - 1].id, "text");
  }

  function clearPaper() {
    lines = [{ id: nextId++, kind: "math", src: "" }];
    changed();
    focusLine(lines[0].id);
  }

  function pin(line: Line) {
    const r = results[line.id];
    if (r?.ok) onpin(line.src, r.answer);
  }

  /** Inserts LaTeX at the caret of the active maths line (or a new line at the end). */
  export function insert(latex: string) {
    let line = lines.find((l) => l.id === activeId && l.kind === "math");
    if (!line) {
      const last = lines[lines.length - 1];
      if (last.kind === "math" && !last.src) {
        line = last;
      } else {
        line = { id: nextId++, kind: "math", src: "" };
        lines.push(line);
      }
    }
    const id = line.id;
    tick().then(() => {
      const f = fields.get(id);
      if (!f) return;
      f.focus();
      f.executeCommand(["insert", latex]);
      const l = lines.find((x) => x.id === id);
      if (l) l.src = f.value;
      changed();
    });
  }

  // ---------- Element wiring ----------

  /** Sets up one maths line's MathLive field. */
  function mathLine(el: HTMLElement, line: Line) {
    const f = el as MathField;
    fields.set(line.id, f);
    f.value = line.src;
    const units = Object.fromEntries(UNIT_SHORTCUTS.map((u) => [u, `\\mathrm{${u}}`]));
    f.inlineShortcuts = { ...f.inlineShortcuts, ...units };
    // " starts and ends upright text on every keyboard layout (MathLive only binds it on
    // some), so "m" types the unit metre: the engine reads \text{m} as a unit.
    f.keybindings = [
      ...f.keybindings,
      { key: "shift+[Quote]", ifMode: "math", command: ["switchMode", "text", "", ""] },
      { key: "shift+[Quote]", ifMode: "text", command: ["switchMode", "math", "", ""] },
    ];
    const onInput = () => {
      line.src = f.value;
      changed();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        pin(line);
      } else if (e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        insertAfter(line.id);
      } else if (e.key === "Backspace" && !f.value) {
        e.preventDefault();
        remove(line.id);
      }
    };
    // MathLive reports when the caret tries to leave the field.
    const onMoveOut = (e: Event) => {
      const dir = (e as CustomEvent).detail?.direction;
      if (dir === "upward" || dir === "backward") move(line.id, -1);
      if (dir === "downward" || dir === "forward") move(line.id, 1);
    };
    f.addEventListener("input", onInput);
    f.addEventListener("keydown", onKey, { capture: true });
    f.addEventListener("move-out", onMoveOut);
    const stopBrackets = watchFieldBrackets(f);
    return {
      destroy() {
        stopBrackets();
        fields.delete(line.id);
        f.removeEventListener("input", onInput);
        f.removeEventListener("keydown", onKey, { capture: true });
        f.removeEventListener("move-out", onMoveOut);
      },
    };
  }

  function noteLine(el: HTMLTextAreaElement, line: Line) {
    notes.set(line.id, el);
    return { destroy: () => notes.delete(line.id) };
  }

  function onNoteKey(e: KeyboardEvent, line: Line) {
    const el = e.currentTarget as HTMLTextAreaElement;
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      insertAfter(line.id);
    } else if (e.key === "Backspace" && !line.src) {
      e.preventDefault();
      remove(line.id);
    } else if (e.key === "ArrowUp" && el.selectionStart === 0) {
      e.preventDefault();
      move(line.id, -1);
    } else if (e.key === "ArrowDown" && el.selectionEnd === line.src.length) {
      e.preventDefault();
      move(line.id, 1);
    }
  }

  function onFocusIn(e: FocusEvent) {
    const el = (e.target as HTMLElement).closest("[data-line]") as HTMLElement | null;
    if (el) activeId = Number(el.dataset.line);
  }

  function setShowSteps(on: boolean) {
    showSteps = on;
    save();
  }

  /** A step note: plain text with $…$ around inline maths. */
  function noteHtml(note: string) {
    const escape = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
    return note
      .split("$")
      .map((part, k) => (k % 2 ? render(part) : escape(part)))
      .join("");
  }

  /** "=" for exact decimals, "≈" for rounded ones. */
  function rel(a: Answer) {
    return a.decimal?.is_exact ? "=" : "≈";
  }

  function showDecimal(a: Answer) {
    if (!a.decimal) return false;
    if (!a.exact_latex) return true;
    return a.exact_latex !== a.decimal.latex;
  }
</script>

<section class="paper" style:padding-bottom="{keyboardHeight}px" onfocusin={onFocusIn} aria-label="Paper">
  <div class="bar">
    <span class="title">Paper</span>
    <div class="seg" role="group" aria-label="Answer style">
      <button class:on={!showSteps} onclick={() => setShowSteps(false)}>Answer</button>
      <button class:on={showSteps} onclick={() => setShowSteps(true)} title="Open the working under every line, not just the one you're on">Steps</button>
    </div>
    <div class="spacer"></div>
    <button class="ghost" class:on={showGraph} onclick={toggleGraph} title="Plot every function of one letter on the Paper">Graph</button>
    <button class="ghost" onclick={addNote} title="Add a text note">+ Note</button>
    <button class="ghost" onclick={clearPaper} title="Start a fresh sheet">Clear</button>
  </div>

  {#if showGraph}
    <div class="graph-wrap" style:flex-basis="{graphShare}%">
      <Graph {series} data={curveRect} bind:view {render} onrange={onRange} />
    </div>
    <div
      class="split"
      role="separator"
      aria-orientation="horizontal"
      aria-label="Resize the graph"
      onpointerdown={startSplit}
    ></div>
  {/if}

  <div class="sheet">
    {#each lines as line, k (line.id)}
      {@const r = results[line.id]}
      <div class="line" class:active={line.id === activeId} class:note={line.kind === "text"} data-line={line.id}>
        <button
          class="kind"
          class:plotted={showGraph && curves[line.id]}
          style:--chip={showGraph && curves[line.id] ? slotColor(slots[line.id] ?? 0) : "transparent"}
          onclick={() => toggleKind(line)}
          title={line.kind === "math" ? "Maths line: click to make it a note" : "Note: click to make it maths"}
          aria-label="Switch line type"
        >
          {line.kind === "math" ? k + 1 : "¶"}
        </button>
        {#if line.kind === "math"}
          <math-field
            use:mathLine={line}
            class="input"
            role="textbox"
            tabindex="0"
            aria-label="Maths line {k + 1}"
            math-virtual-keyboard-policy="auto"
            smart-fence
          ></math-field>
          <div class="answer">
            {#if r?.ok}
              {@const a = r.answer}
              {#if a.exact_latex}
                <span class="value"><span class="rel">{a.defines ? "≔" : "="}</span>{@html render(a.exact_latex)}</span>
              {/if}
              {#if showDecimal(a)}
                <span class="value decimal"><span class="rel">{rel(a)}</span>{@html render(a.decimal!.latex)}</span>
              {/if}
              <button class="pin" onclick={() => pin(line)} title="Pin to history (Ctrl+Enter)" aria-label="Pin to history">📌</button>
            {:else if r && line.src.trim()}
              <span class="error" title={r.error.message}>!</span>
            {/if}
          </div>
          {#if (line.id === activeId || showSteps) && r?.ok}
            <!-- The worked view: the working as a chain, then the final answer, large. -->
            {@const a = r.answer}
            {@const methods = a.methods ?? []}
            {@const pick = Math.min(chosenMethod[line.id] ?? 0, Math.max(0, methods.length - 1))}
            <div class="worked">
              {#if methods.length}
                {#if methods.length > 1}
                  <div class="methods" role="tablist" aria-label="Method">
                    {#each methods as meth, k}
                      <button
                        role="tab"
                        aria-selected={k === pick}
                        class:on={k === pick}
                        onclick={() => (chosenMethod[line.id] = k)}>{meth.name}</button
                      >
                    {/each}
                  </div>
                {:else}
                  <div class="method-name">{methods[0].name}</div>
                {/if}
                <div class="chain">
                  {#each workedRows(methods[pick].steps) as row}
                    {#if row.note}
                      <div class="chain-note" class:aside={row.aside}>{@html noteHtml(row.note)}</div>
                    {/if}
                    {#if row.latex}
                      <div class="chain-row" class:cont={row.rel === "="} class:aside={row.aside}>
                        <span class="chain-rel">{row.rel}</span>
                        <span class="chain-math">{@html render(`\\displaystyle ${row.latex}`)}</span>
                      </div>
                    {/if}
                  {/each}
                </div>
              {/if}
              {#if a.exact_latex || a.decimal}
                <div class="final" aria-label="Answer">
                  {#if a.exact_latex}
                    <div class="final-exact">
                      <span class="final-rel">{a.defines ? "≔" : "="}</span>
                      <span class="final-math">{@html render(`\\displaystyle ${a.exact_latex}`)}</span>
                    </div>
                  {/if}
                  {#if showDecimal(a)}
                    <div class="final-decimal">
                      <span class="final-rel">{rel(a)}</span>{@html render(a.decimal!.latex)}
                    </div>
                  {/if}
                </div>
              {/if}
            </div>
          {:else if line.id === activeId && r && !r.ok && line.src.trim()}
            <div class="worked">
              <div class="final error-card" role="alert">{r.error.message}</div>
            </div>
          {/if}
        {:else}
          <textarea
            use:noteLine={line}
            class="input text"
            rows="1"
            placeholder="Note"
            bind:value={line.src}
            oninput={changed}
            onkeydown={(e) => onNoteKey(e, line)}
          ></textarea>
        {/if}
      </div>
    {/each}
  </div>
</section>

<style>
  /* The Paper is the page: it fills everything under the header. */
  .paper {
    position: relative;
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }

  .bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 20px 8px;
    width: 100%;
    max-width: 960px;
    margin: 0 auto;
  }

  .title {
    font-weight: 700;
    font-size: 20px;
    letter-spacing: -0.2px;
    color: var(--ink);
    margin-right: 6px;
  }

  .spacer {
    flex: 1;
  }

  /* Pill switch, like Month | Year. */
  .seg {
    display: flex;
    background: var(--panel);
    border-radius: 999px;
    padding: 4px;
    gap: 2px;
  }

  .seg button {
    border: none;
    background: none;
    padding: 5px 14px;
    border-radius: 999px;
    font-size: 13.5px;
    font-weight: 600;
    color: var(--muted);
    cursor: pointer;
    transition: background 140ms, color 140ms;
  }

  .seg button:hover:not(.on) {
    color: var(--ink);
  }

  .seg button.on {
    background: var(--ink);
    color: var(--bg);
  }

  .seg button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }

  .ghost {
    background: var(--panel);
    border: none;
    border-radius: 999px;
    padding: 7px 14px;
    font-size: 13.5px;
    font-weight: 600;
    color: var(--muted);
    cursor: pointer;
    transition: background 140ms, color 140ms;
  }

  .ghost:hover {
    color: var(--ink);
    background: var(--panel-2);
  }

  .ghost.on {
    background: var(--accent);
    color: var(--accent-ink);
  }

  .sheet {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 6px 20px 28px;
    width: 100%;
    max-width: 960px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .line {
    display: grid;
    grid-template-columns: 30px minmax(0, 1fr) auto;
    align-items: center;
    gap: 6px 12px;
    padding: 6px 12px;
    border-radius: var(--radius-sm);
    transition: background 160ms;
  }

  .line:hover:not(.active) {
    background: color-mix(in srgb, var(--panel) 55%, transparent);
  }

  /* The line you're on becomes a card and opens its working. */
  .line.active {
    background: var(--panel);
    border-radius: var(--radius);
    padding: 12px 16px 16px;
    margin: 6px 0;
    box-shadow: var(--shadow);
  }

  .line.note {
    grid-template-columns: 30px minmax(0, 1fr);
  }

  .kind {
    background: none;
    border: none;
    color: var(--muted);
    font-size: 12.5px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    cursor: pointer;
    opacity: 0.75;
    padding: 0;
    height: 26px;
    border-radius: 999px;
  }

  .kind:hover {
    color: var(--accent-strong);
    opacity: 1;
  }

  .line.active .kind {
    background: var(--panel-2);
    color: var(--ink);
    opacity: 1;
  }

  /* A plotted line wears its curve's colour as a dot beside its number. */
  .kind.plotted {
    opacity: 1;
    color: var(--ink);
    box-shadow: inset 0 0 0 2px var(--chip);
  }

  .graph-wrap {
    flex: 0 0 58%;
    min-height: 180px;
    padding: 0 20px 4px;
    width: 100%;
    max-width: 1200px;
    margin: 0 auto;
  }

  .split {
    flex: none;
    height: 12px;
    cursor: ns-resize;
    touch-action: none;
    position: relative;
  }

  .split::after {
    content: "";
    position: absolute;
    left: 50%;
    top: 4px;
    width: 48px;
    height: 4px;
    margin-left: -24px;
    border-radius: 2px;
    background: var(--line);
  }

  .split:hover::after {
    background: var(--muted);
  }

  .input {
    min-width: 0;
    font-size: 22px;
    min-height: 36px;
  }

  .text {
    font: inherit;
    font-size: 16px;
    color: var(--muted);
    background: none;
    border: none;
    outline: none;
    resize: none;
    field-sizing: content;
    padding: 6px 0;
    line-height: 1.5;
  }

  /* The short answer beside the line. */
  .answer {
    display: flex;
    align-items: baseline;
    gap: 12px;
    justify-content: flex-end;
    max-width: 50vw;
    overflow-x: auto;
    overflow-y: hidden;
    font-size: 19px;
  }

  .value {
    display: flex;
    gap: 6px;
    align-items: baseline;
    white-space: nowrap;
  }

  .value.decimal {
    color: var(--accent-strong);
    font-size: 16px;
  }

  .rel {
    color: var(--muted);
    font-size: 0.8em;
  }

  .pin {
    background: none;
    border: none;
    cursor: pointer;
    font-size: 13px;
    opacity: 0;
    transition: opacity 120ms;
    padding: 0 2px;
  }

  .line:hover .pin,
  .line.active .pin {
    opacity: 0.6;
  }

  .pin:hover {
    opacity: 1 !important;
  }

  .error {
    display: inline-grid;
    place-items: center;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: color-mix(in srgb, var(--err) 18%, transparent);
    color: var(--err);
    font-size: 13px;
    font-weight: 700;
  }

  /* ---------- The worked view ---------- */

  .worked {
    grid-column: 2 / -1;
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding-top: 6px;
    animation: open 180ms ease-out;
  }

  @keyframes open {
    from {
      opacity: 0;
      transform: translateY(-4px);
    }
  }

  .methods {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-self: flex-start;
    background: var(--panel-2);
    border-radius: 999px;
    padding: 4px;
  }

  .methods button {
    background: none;
    border: none;
    border-radius: 999px;
    padding: 4px 12px;
    font-size: 12.5px;
    font-weight: 600;
    color: var(--muted);
    cursor: pointer;
  }

  .methods button:hover:not(.on) {
    color: var(--ink);
  }

  .methods button.on {
    background: var(--ink);
    color: var(--bg);
  }

  .method-name {
    font-size: 12px;
    font-weight: 700;
    color: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.6px;
  }

  /* Rows of working, each "= …" under the one before, like on paper. */
  .chain {
    display: grid;
    grid-template-columns: 22px minmax(0, 1fr);
    column-gap: 8px;
    row-gap: 2px;
    overflow-x: auto;
    padding: 2px 0 4px;
  }

  .chain-note {
    grid-column: 2;
    margin-top: 8px;
    font-size: 12.5px;
    color: var(--muted);
    line-height: 1.5;
  }

  .chain-note :global(.ML__latex) {
    font-size: 0.95em;
  }

  .chain-row {
    display: contents;
  }

  .chain-rel {
    grid-column: 1;
    align-self: center;
    text-align: center;
    font-size: 20px;
    color: var(--muted);
  }

  .chain-math {
    grid-column: 2;
    font-size: 21px;
    padding: 3px 0;
    white-space: nowrap;
  }

  /* A side calculation inside a chain: indented and quieter, so the chain reads past it. */
  .chain-note.aside {
    padding-left: 22px;
  }

  .chain-row.aside .chain-math {
    margin-left: 14px;
    padding: 2px 0 2px 10px;
    border-left: 2px solid var(--line);
    font-size: 17px;
    color: var(--muted);
  }

  /* The final answer, large, in its own card. */
  .final {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 16px 20px;
    border-radius: var(--radius);
    background: var(--panel-2);
    overflow-x: auto;
  }

  .final-exact {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 34px;
    font-weight: 600;
    line-height: 1.25;
    white-space: nowrap;
  }

  .final-decimal {
    display: flex;
    align-items: baseline;
    gap: 8px;
    font-size: 18px;
    color: var(--accent-strong);
    white-space: nowrap;
  }

  .final-rel {
    color: var(--muted);
    font-size: 0.75em;
    font-weight: 400;
  }

  .error-card {
    background: color-mix(in srgb, var(--err) 12%, var(--panel));
    color: var(--err);
    font-size: 14.5px;
    line-height: 1.5;
  }

  @media (max-width: 560px) {
    .bar,
    .sheet {
      padding-left: 12px;
      padding-right: 12px;
    }
    .input {
      font-size: 19px;
    }
    .answer {
      grid-column: 2 / -1;
      max-width: none;
      font-size: 17px;
    }
    .line {
      grid-template-columns: 22px minmax(0, 1fr);
    }
    .final-exact {
      font-size: 27px;
    }
    .chain-math {
      font-size: 18px;
    }
  }
</style>
