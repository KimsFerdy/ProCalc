<script lang="ts">
  import { onMount, tick } from "svelte";
  import { evaluate, migrateAnswer, type Answer, type Settings } from "$lib/engine";

  interface Entry {
    id: number;
    input: string;
    answer: Answer | null;
    error: string | null;
  }

  const DIGIT_CHOICES = [16, 32, 50, 100, 250, 1000];
  const STORE_KEY = "procalc.v1";

  let settings = $state<Settings>({ digits: 32, angle: "radians" });
  let history = $state<Entry[]>([]);
  let preview = $state<Answer | null>(null);
  let inputError = $state<string | null>(null);
  let copied = $state<number | null>(null);
  let ready = $state(false);

  type MathField = HTMLElement & { value: string; focus(): void; executeCommand(c: unknown): boolean };
  let field = $state<MathField | undefined>();
  let feed = $state<HTMLElement | undefined>();
  let toMarkup: (latex: string) => string = (l) => l;
  let nextId = 1;
  let previewTimer: ReturnType<typeof setTimeout> | undefined;
  let previewSeq = 0;

  function load() {
    try {
      const raw = localStorage.getItem(STORE_KEY);
      if (!raw) return;
      const saved = JSON.parse(raw);
      if (saved.settings) settings = { ...settings, ...saved.settings };
      if (Array.isArray(saved.history)) {
        history = saved.history
          .slice(-200)
          .map((h: Entry) => ({ ...h, answer: migrateAnswer(h.answer) }));
        nextId = Math.max(0, ...history.map((h) => h.id)) + 1;
      }
    } catch {
      /* first run, or storage unavailable */
    }
  }

  function save() {
    try {
      localStorage.setItem(STORE_KEY, JSON.stringify({ settings, history: history.slice(-200) }));
    } catch {
      /* storage unavailable: history just won't persist */
    }
  }

  onMount(() => {
    load();
    (async () => {
      const ml = await import("mathlive");
      // Fonts come from the bundled CSS; don't let MathLive fetch them itself.
      ml.MathfieldElement.fontsDirectory = null;
      ml.MathfieldElement.soundsDirectory = null;
      toMarkup = (latex) => ml.convertLatexToMarkup(latex);
      ready = true;
      await tick();
      field?.focus();
      scrollToEnd();
    })();
  });

  function render(latex: string) {
    return toMarkup(latex);
  }

  function schedulePreview() {
    inputError = null;
    clearTimeout(previewTimer);
    previewTimer = setTimeout(runPreview, 90);
  }

  async function runPreview() {
    const latex = field?.value.trim() ?? "";
    const seq = ++previewSeq;
    if (!latex) {
      preview = null;
      return;
    }
    const r = await evaluate(latex, settings);
    if (seq !== previewSeq) return;
    // While typing, keep quiet about errors (half-typed input is normal).
    preview = r.ok ? r.answer : null;
  }

  async function commit() {
    const latex = field?.value.trim() ?? "";
    if (!latex) return;
    const r = await evaluate(latex, settings);
    if (!r.ok) {
      inputError = r.error.message;
      return;
    }
    history = [...history, { id: nextId++, input: latex, answer: r.answer, error: null }];
    save();
    if (field) field.value = "";
    preview = null;
    inputError = null;
    scrollToEnd();
  }

  /** Runs expand/factor on a history entry and adds the result as a new entry. */
  async function applyOp(entry: Entry, op: "expand" | "factor") {
    const latex = `\\operatorname{${op}}\\left(${entry.input}\\right)`;
    const r = await evaluate(latex, settings);
    if (!r.ok) {
      inputError = r.error.message;
      return;
    }
    history = [...history, { id: nextId++, input: latex, answer: r.answer, error: null }];
    save();
    scrollToEnd();
  }

  async function scrollToEnd() {
    await tick();
    feed?.scrollTo({ top: feed.scrollHeight, behavior: "smooth" });
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      commit();
    }
  }

  function reuse(latex: string) {
    if (!field) return;
    field.executeCommand(["insert", latex]);
    field.focus();
    schedulePreview();
  }

  async function copy(entry: Entry) {
    if (!entry.answer) return;
    try {
      await navigator.clipboard.writeText(entry.answer.decimal?.text ?? entry.answer.exact_latex ?? "");
      copied = entry.id;
      setTimeout(() => (copied = copied === entry.id ? null : copied), 1200);
    } catch {
      /* clipboard blocked */
    }
  }

  function clearHistory() {
    history = [];
    save();
  }

  function setAngle(angle: Settings["angle"]) {
    settings.angle = angle;
    save();
    schedulePreview();
  }

  function setDigits(e: Event) {
    settings.digits = Number((e.target as HTMLSelectElement).value);
    save();
    schedulePreview();
  }

  /** "=" for exact decimals, "≈" for rounded ones. */
  function rel(a: Answer) {
    return a.decimal?.is_exact ? "=" : "≈";
  }

  /** The decimal line is redundant when the exact answer is already that same integer or decimal. */
  function showDecimal(a: Answer) {
    if (!a.decimal) return false;
    if (!a.exact_latex) return true;
    return a.exact_latex !== a.decimal.latex;
  }
</script>

<div class="app">
  <header>
    <div class="brand">
      <span class="logo">∑</span>
      <span class="name">ProCalc</span>
    </div>
    <div class="controls">
      <div class="seg" role="group" aria-label="Angle unit">
        <button class:on={settings.angle === "degrees"} onclick={() => setAngle("degrees")}>DEG</button>
        <button class:on={settings.angle === "radians"} onclick={() => setAngle("radians")}>RAD</button>
      </div>
      <label class="digits">
        <span>digits</span>
        <select value={settings.digits} onchange={setDigits}>
          {#each DIGIT_CHOICES as d}
            <option value={d}>{d}</option>
          {/each}
        </select>
      </label>
      {#if history.length}
        <button class="ghost" onclick={clearHistory} title="Clear history">Clear</button>
      {/if}
    </div>
  </header>

  <main bind:this={feed}>
    {#if history.length === 0}
      <div class="empty">
        <p class="hello">Type maths the way you write it.</p>
        <ul>
          <li><code>sqrt</code> → √, <code>/</code> → fraction, <code>^</code> → power, <code>pi</code> → π</li>
          <li>Results stay <b>exact</b> when they can (0.1 + 0.2 = 3/10), with up to 1000 digits otherwise.</li>
          <li>Algebra works too: <code>x+x</code>, <code>factor(x^2-1)</code>, <code>expand((x+1)^3)</code></li>
          <li><kbd>Enter</kbd> to calculate · click a result to reuse it</li>
        </ul>
      </div>
    {/if}

    {#if ready}
      {#each history as entry (entry.id)}
        <article class="entry">
          <div class="input">{@html render(entry.input)}</div>
          {#if entry.answer}
            {@const a = entry.answer}
            <div class="result">
              {#if a.exact_latex}
                <button class="value exact" onclick={() => reuse(a.exact_latex!)} title="Insert into input">
                  <span class="rel">=</span>{@html render(a.exact_latex)}
                </button>
              {/if}
              {#if showDecimal(a)}
                <button class="value decimal" onclick={() => reuse(a.decimal!.latex)} title="Insert into input">
                  <span class="rel">{rel(a)}</span>{@html render(a.decimal!.latex)}
                </button>
              {/if}
            </div>
            <div class="actions">
              <button class="ghost small" onclick={() => copy(entry)}>
                {copied === entry.id ? "Copied" : "Copy"}
              </button>
              {#if a.symbolic}
                <button class="ghost small" onclick={() => applyOp(entry, "expand")}>Expand</button>
                <button class="ghost small" onclick={() => applyOp(entry, "factor")}>Factor</button>
              {/if}
            </div>
          {/if}
        </article>
      {/each}
    {/if}
  </main>

  <footer>
    <div class="composer" class:has-error={inputError}>
      {#if ready}
        <math-field
          bind:this={field}
          oninput={schedulePreview}
          onkeydown={onKeydown}
          role="textbox"
          tabindex="0"
          aria-label="Expression"
          math-virtual-keyboard-policy="auto"
          smart-fence
        ></math-field>
      {/if}
      <button class="go" onclick={commit} aria-label="Calculate">=</button>
    </div>
    <div class="status">
      {#if inputError}
        <span class="error">{inputError}</span>
      {:else if preview}
        <span class="preview">
          {#if preview.exact_latex}
            <span class="rel">=</span>{@html render(preview.exact_latex)}
          {:else if preview.decimal}
            <span class="rel">{rel(preview)}</span>{@html render(preview.decimal.latex)}
          {/if}
        </span>
      {/if}
    </div>
  </footer>
</div>

<style>
  .app {
    display: grid;
    grid-template-rows: auto 1fr auto;
    height: 100vh;
    height: 100dvh;
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 12px 16px;
    border-bottom: 1px solid var(--line);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    font-weight: 650;
    letter-spacing: 0.2px;
  }

  .logo {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: 9px;
    background: var(--accent);
    color: var(--accent-ink);
    font-size: 18px;
  }

  .controls {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .seg {
    display: flex;
    background: var(--panel-2);
    border: 1px solid var(--line);
    border-radius: 999px;
    padding: 2px;
  }

  .seg button {
    border: none;
    background: none;
    padding: 4px 12px;
    border-radius: 999px;
    font-size: 12.5px;
    font-weight: 600;
    letter-spacing: 0.6px;
    color: var(--muted);
    cursor: pointer;
  }

  .seg button.on {
    background: var(--accent);
    color: var(--accent-ink);
  }

  .digits {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12.5px;
    color: var(--muted);
  }

  .digits select {
    background: var(--panel-2);
    color: var(--ink);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 4px 6px;
    font: inherit;
  }

  .ghost {
    background: none;
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 4px 10px;
    font-size: 12.5px;
    color: var(--muted);
    cursor: pointer;
  }

  .ghost:hover {
    color: var(--ink);
    border-color: var(--muted);
  }

  .ghost.small {
    padding: 2px 8px;
    font-size: 12px;
  }

  main {
    overflow-y: auto;
    padding: 20px 16px 8px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    width: 100%;
    max-width: 920px;
    margin: 0 auto;
  }

  .empty {
    margin: auto 0;
    color: var(--muted);
    line-height: 1.7;
  }

  .empty .hello {
    font-size: 22px;
    color: var(--ink);
    margin: 0 0 8px;
  }

  .empty ul {
    margin: 0;
    padding-left: 18px;
  }

  code,
  kbd {
    font-family: var(--font-mono);
    font-size: 0.9em;
    background: var(--panel-2);
    border: 1px solid var(--line);
    border-radius: 5px;
    padding: 0 5px;
  }

  .entry {
    position: relative;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 14px 16px 12px;
  }

  .entry .input {
    color: var(--muted);
    font-size: 17px;
    overflow-x: auto;
  }

  .result {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 2px;
    margin-top: 6px;
  }

  .value {
    display: flex;
    align-items: baseline;
    gap: 10px;
    max-width: 100%;
    overflow-x: auto;
    background: none;
    border: none;
    padding: 2px 4px;
    border-radius: 8px;
    cursor: pointer;
    text-align: right;
  }

  .value:hover {
    background: var(--panel-2);
  }

  .value.exact {
    font-size: 26px;
    color: var(--ink);
  }

  .value.decimal {
    font-size: 18px;
    color: var(--accent);
  }

  .rel {
    color: var(--muted);
    font-size: 0.8em;
  }

  .actions {
    position: absolute;
    left: 12px;
    bottom: 10px;
    display: flex;
    gap: 6px;
    opacity: 0.45;
    transition: opacity 120ms;
  }

  .entry:hover .actions,
  .entry:focus-within .actions {
    opacity: 1;
  }

  footer {
    border-top: 1px solid var(--line);
    padding: 12px 16px 14px;
    width: 100%;
    max-width: 920px;
    margin: 0 auto;
  }

  .composer {
    display: flex;
    align-items: center;
    gap: 10px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: var(--radius);
    padding: 10px 10px 10px 16px;
    transition: border-color 120ms;
  }

  .composer:focus-within {
    border-color: var(--accent);
  }

  .composer.has-error {
    border-color: var(--err);
  }

  .composer math-field {
    flex: 1;
    min-width: 0;
    font-size: 26px;
    min-height: 44px;
  }

  .go {
    flex: none;
    width: 46px;
    height: 46px;
    border-radius: 12px;
    border: none;
    background: var(--accent);
    color: var(--accent-ink);
    font-size: 24px;
    font-weight: 700;
    cursor: pointer;
  }

  .status {
    min-height: 34px;
    padding: 6px 6px 0;
    display: flex;
    justify-content: flex-end;
    font-size: 18px;
    overflow-x: auto;
  }

  .preview {
    display: flex;
    gap: 8px;
    align-items: baseline;
    color: var(--muted);
  }

  .error {
    color: var(--err);
    font-size: 14px;
  }

  @media (max-width: 560px) {
    .digits span {
      display: none;
    }
    .value.exact {
      font-size: 22px;
    }
    .composer math-field {
      font-size: 22px;
    }
  }
</style>
