<script lang="ts">
  import { onMount, tick } from "svelte";
  import { evaluate, migrateAnswer, type Answer, type Settings } from "$lib/engine";
  import Paper from "$lib/Paper.svelte";
  import Geometry from "$lib/Geometry.svelte";
  import UnitCircle from "$lib/UnitCircle.svelte";
  import { ask } from "@tauri-apps/plugin-dialog";
  import UpdateDialog from "$lib/UpdateDialog.svelte";
  import { formatBytes, updates } from "$lib/updates.svelte";
  import { colorBracketsInMarkup } from "$lib/brackets";
  import { theme, type ThemeChoice } from "$lib/theme.svelte";

  const THEMES: { id: ThemeChoice; name: string }[] = [
    { id: "system", name: "System" },
    { id: "light", name: "Light" },
    { id: "dark", name: "Dark" },
  ];
  import {
    chooseOpenPath,
    chooseSavePath,
    forgetRecent,
    makeProject,
    projectName,
    readProjectFile,
    recentProjects,
    rememberRecent,
    writeProjectFile,
    type Project,
    type ProjectSettings,
    type Workspace,
  } from "$lib/project";

  interface Entry {
    id: number;
    input: string;
    answer: Answer | null;
    error: string | null;
  }

  const DIGIT_CHOICES = [16, 32, 50, 100, 250, 1000];
  const STORE_KEY = "procalc.v1";
  const MODE_KEY = "procalc.mode";
  const WORKSPACE_KEY = "procalc.workspace";

  /** The subject areas ProCalc covers; only the ready ones can be chosen. */
  const MODES = [
    { id: "math", name: "Math", icon: "∑", blurb: "Algebra, calculus, matrices, statistics", ready: true },
    { id: "physics", name: "Physics", icon: "⚛", blurb: "Mechanics, waves, electricity", ready: false },
    { id: "engineering", name: "Engineering", icon: "⚙", blurb: "Circuits, structures, signals", ready: false },
    { id: "finance", name: "Finance", icon: "%", blurb: "Interest, loans, investments", ready: false },
  ];

  /** Math's workspaces: the Paper for everything, and drawing ones for what needs a picture. */
  const WORKSPACES: { id: Workspace; name: string; icon: string; blurb: string; ready: boolean }[] = [
    { id: "paper", name: "Paper", icon: "✎", blurb: "Lines of maths, steps and the graph", ready: true },
    { id: "geometry", name: "Geometry", icon: "△", blurb: "Draw a shape, give what you know, get the rest", ready: true },
    { id: "unit-circle", name: "Unit circle", icon: "◔", blurb: "sin, cos and tan on the circle", ready: true },
  ];

  let settings = $state<Settings>({ digits: 32, angle: "radians" });
  let history = $state<Entry[]>([]);
  let copied = $state<number | null>(null);
  let notice = $state<string | null>(null);
  let ready = $state(false);
  /** The history drawer, sliding in from the right. */
  let historyOpen = $state(false);
  let justPinned = $state(false);
  let mode = $state("math");
  let modeOpen = $state(false);
  const currentMode = $derived(MODES.find((m) => m.id === mode) ?? MODES[0]);

  function chooseMode(id: string) {
    const m = MODES.find((x) => x.id === id);
    if (!m?.ready) return;
    mode = id;
    modeOpen = false;
    try {
      localStorage.setItem(MODE_KEY, id);
    } catch {
      /* storage unavailable: the mode just won't persist */
    }
  }

  let workspace = $state<Workspace>("paper");
  let geometry = $state<Geometry | undefined>();
  let unitCircle = $state<UnitCircle | undefined>();
  const currentWorkspace = $derived(WORKSPACES.find((w) => w.id === workspace) ?? WORKSPACES[0]);

  function chooseWorkspace(id: string) {
    const w = WORKSPACES.find((x) => x.id === id);
    if (!w?.ready) return;
    workspace = id as Workspace;
    modeOpen = false;
    try {
      localStorage.setItem(WORKSPACE_KEY, id);
    } catch {
      /* storage unavailable: the workspace just won't persist */
    }
  }

  let paper = $state<Paper | undefined>();
  let tipsDialog = $state<HTMLDialogElement | undefined>();
  let feed = $state<HTMLElement | undefined>();
  let toMarkup: (latex: string) => string = (l) => l;
  let nextId = 1;

  function load() {
    try {
      const raw = localStorage.getItem(STORE_KEY);
      if (!raw) return;
      const savedMode = localStorage.getItem(MODE_KEY);
      if (savedMode && MODES.some((m) => m.id === savedMode && m.ready)) mode = savedMode;
      const savedWorkspace = localStorage.getItem(WORKSPACE_KEY);
      if (savedWorkspace && WORKSPACES.some((w) => w.id === savedWorkspace && w.ready)) workspace = savedWorkspace as Workspace;
      const saved = JSON.parse(raw);
      if (saved.settings) settings = { ...settings, ...saved.settings };
      if (typeof saved.lengthUnit === "string") lengthUnit = saved.lengthUnit;
      if (typeof saved.projectPath === "string") projectPath = saved.projectPath;
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
      localStorage.setItem(STORE_KEY, JSON.stringify({ settings, lengthUnit, projectPath, history: history.slice(-200) }));
    } catch {
      /* storage unavailable: history just won't persist */
    }
  }

  onMount(() => {
    load();
    recent = recentProjects();
    updates.start();
    (async () => {
      const ml = await import("mathlive");
      // Fonts come from the bundled CSS; don't let MathLive fetch them itself.
      ml.MathfieldElement.fontsDirectory = null;
      ml.MathfieldElement.soundsDirectory = null;
      toMarkup = (latex) => ml.convertLatexToMarkup(latex);
      ready = true;
      // The Paper appears now and loads its lines: that is the saved state to compare with.
      await tick();
      savedSnapshot = currentSnapshot = snapshot();
      baseline = true;
      scrollToEnd();
    })();
  });

  /** Maths as markup, with its brackets colour-coded by nesting level. */
  function render(latex: string) {
    return colorBracketsInMarkup(toMarkup(latex));
  }

  /** Definition lines in history, oldest first: Expand/Factor on an entry can use them. */
  function defs() {
    return history.filter((h) => h.answer?.defines).map((h) => h.input);
  }

  /** A Paper line pinned to the history (Ctrl+Enter or its pin button); the count pulses. */
  function pinned(latex: string, answer: Answer) {
    history = [...history, { id: nextId++, input: latex, answer, error: null }];
    save();
    justPinned = true;
    setTimeout(() => (justPinned = false), 700);
    scrollToEnd();
  }

  /** Runs expand/factor on a history entry and adds the result as a new entry. */
  async function applyOp(entry: Entry, op: "expand" | "factor") {
    const latex = `\\operatorname{${op}}\\left(${entry.input}\\right)`;
    const r = await evaluate(latex, defs(), settings);
    if (!r.ok) {
      flash(r.error.message);
      return;
    }
    history = [...history, { id: nextId++, input: latex, answer: r.answer, error: null }];
    save();
    scrollToEnd();
  }

  function flash(message: string) {
    notice = message;
    setTimeout(() => (notice = notice === message ? null : notice), 3000);
  }

  async function scrollToEnd() {
    await tick();
    feed?.scrollTo({ top: feed.scrollHeight, behavior: "smooth" });
  }

  function reuse(latex: string) {
    paper?.insert(latex);
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
    projectChanged();
  }

  function setDigits(e: Event) {
    settings.digits = Number((e.target as HTMLSelectElement).value);
    save();
    projectChanged();
  }

  // ---------- Projects ----------

  /** The project's file, once it has been saved or opened. */
  let projectPath = $state<string | null>(null);
  /** What was last written to (or read from) the file, to tell whether anything changed. */
  let savedSnapshot = $state("");
  let currentSnapshot = $state("");
  const dirty = $derived(currentSnapshot !== savedSnapshot);
  let fileOpen = $state(false);
  let recent = $state<string[]>([]);
  let toast = $state<string | null>(null);
  let autosaveTimer: ReturnType<typeof setTimeout> | undefined;

  function projectSettings(): ProjectSettings {
    return { digits: settings.digits, angle: settings.angle, lengthUnit };
  }

  function currentProject(): Project | null {
    const state = paper?.getState();
    return state ? makeProject(mode, projectSettings(), workspace, state, geometry?.getState() ?? null, unitCircle?.getState() ?? null) : null;
  }

  /** The project without its timestamp or open workspace: equal snapshots mean nothing to save. */
  function snapshot(): string {
    const p = currentProject();
    return p ? JSON.stringify({ ...p, saved: "", workspace: "" }) : "";
  }

  function show(message: string) {
    toast = message;
    setTimeout(() => (toast = toast === message ? null : toast), 4000);
  }

  /** Changes count once the startup state is in place. */
  let baseline = false;

  /** The Paper or a setting changed: note it, and autosave when the project has a file. */
  function projectChanged() {
    if (!baseline) return;
    currentSnapshot = snapshot();
    if (projectPath && dirty) {
      clearTimeout(autosaveTimer);
      autosaveTimer = setTimeout(() => saveProject(false), 1500);
    }
  }

  async function writeTo(path: string) {
    const p = currentProject();
    if (!p) return;
    await writeProjectFile(path, p);
    projectPath = path;
    savedSnapshot = currentSnapshot = snapshot();
    recent = rememberRecent(path);
    save();
  }

  /** Save (to the project's file, asking for one the first time) or Save as. */
  async function saveProject(saveAs: boolean) {
    fileOpen = false;
    try {
      let path = saveAs ? null : projectPath;
      if (!path) path = await chooseSavePath(mode, projectPath);
      if (!path) return;
      await writeTo(path);
      if (saveAs || !projectPath) show(`Saved ${projectName(path)}`);
    } catch (e) {
      show(String(e instanceof Error ? e.message : e));
    }
  }

  async function confirmDiscard(): Promise<boolean> {
    if (!dirty) return true;
    return await ask("Your changes to this project haven't been saved. Continue and lose them?", {
      title: "Unsaved changes",
      kind: "warning",
      okLabel: "Lose changes",
      cancelLabel: "Keep editing",
    });
  }

  /** Before an update restarts the app: save a project that has a file, else ask. */
  async function beforeUpdate(): Promise<boolean> {
    if (!dirty) return true;
    if (projectPath) {
      clearTimeout(autosaveTimer);
      try {
        await writeTo(projectPath);
        return true;
      } catch (e) {
        show(String(e instanceof Error ? e.message : e));
        return false;
      }
    }
    return await ask("This project hasn't been saved. Update and restart anyway, losing it?", {
      title: "Unsaved changes",
      kind: "warning",
      okLabel: "Update anyway",
      cancelLabel: "Not now",
    });
  }

  async function newProject() {
    fileOpen = false;
    if (!(await confirmDiscard())) return;
    paper?.setState({ lines: [{ kind: "math", src: "" }], showSteps: false, showGraph: false, graphShare: 58, view: { xMin: -10, xMax: 10, yMin: -6, yMax: 6 } });
    geometry?.setState(null);
    unitCircle?.setState(null);
    projectPath = null;
    savedSnapshot = currentSnapshot = snapshot();
    save();
  }

  async function openProject(path?: string) {
    fileOpen = false;
    if (!(await confirmDiscard())) return;
    try {
      const chosen = path ?? (await chooseOpenPath());
      if (!chosen) return;
      const p = await readProjectFile(chosen);
      settings.digits = p.settings.digits;
      settings.angle = p.settings.angle;
      lengthUnit = p.settings.lengthUnit;
      mode = p.subject;
      paper?.setState(p.paper);
      geometry?.setState(p.geometry);
      unitCircle?.setState(p.unitCircle);
      chooseWorkspace(p.workspace);
      projectPath = chosen;
      savedSnapshot = currentSnapshot = snapshot();
      recent = rememberRecent(chosen);
      save();
    } catch (e) {
      const message = String(e instanceof Error ? e.message : e);
      // A recent file that has gone away drops off the list.
      if (path && message.startsWith("Couldn't open")) recent = forgetRecent(path);
      show(message);
    }
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      modeOpen = false;
      fileOpen = false;
    }
    if (!(e.ctrlKey || e.metaKey)) return;
    const key = e.key.toLowerCase();
    if (key === "s") {
      e.preventDefault();
      saveProject(e.shiftKey);
    } else if (key === "o") {
      e.preventDefault();
      openProject();
    } else if (key === "n") {
      e.preventDefault();
      newProject();
    }
  }

  // ---------- Settings ----------

  const LENGTH_UNITS = [
    { id: "mm", name: "millimetres (mm)" },
    { id: "cm", name: "centimetres (cm)" },
    { id: "m", name: "metres (m)" },
    { id: "km", name: "kilometres (km)" },
    { id: "in", name: "inches (in)" },
    { id: "ft", name: "feet (ft)" },
    { id: "yd", name: "yards (yd)" },
    { id: "mi", name: "miles (mi)" },
  ];
  /** The unit for lengths in geometry and measurements (areas and volumes follow). */
  let lengthUnit = $state("m");
  let settingsDialog = $state<HTMLDialogElement | undefined>();

  function setLengthUnit(e: Event) {
    lengthUnit = (e.target as HTMLSelectElement).value;
    save();
    projectChanged();
  }

  function lastCheckedText(t: number) {
    const mins = Math.round((Date.now() - t) / 60000);
    if (mins < 1) return "just now";
    if (mins < 60) return `${mins} minute${mins === 1 ? "" : "s"} ago`;
    const hours = Math.round(mins / 60);
    if (hours < 24) return `${hours} hour${hours === 1 ? "" : "s"} ago`;
    return new Date(t).toLocaleDateString();
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

<svelte:window onkeydown={onKeydown} />

<div class="app">
  <header>
    <div class="brand-wrap">
      <button
        class="brand"
        aria-haspopup="menu"
        aria-expanded={modeOpen}
        onclick={() => (modeOpen = !modeOpen)}
        title="Choose a subject"
      >
        <span class="logo">{currentMode.icon}</span>
        <span class="name">ProCalc</span>
        <span class="mode-chip">{currentMode.name}{#if mode === "math" && workspace !== "paper"}{" · "}{currentWorkspace.name}{/if}<span class="caret" class:up={modeOpen}>▾</span></span>
      </button>
      {#if modeOpen}
        <!-- A click anywhere else closes the menu. -->
        <div class="menu-backdrop" role="presentation" onclick={() => (modeOpen = false)}></div>
        <div class="mode-menu" role="menu" aria-label="Subjects">
          {#each MODES as m}
            <button
              class="mode-item"
              class:active={m.id === mode}
              role="menuitemradio"
              aria-checked={m.id === mode}
              aria-disabled={!m.ready}
              disabled={!m.ready}
              onclick={() => chooseMode(m.id)}
            >
              <span class="mode-icon">{m.icon}</span>
              <span class="mode-text">
                <span class="mode-name">{m.name}</span>
                <span class="mode-blurb">{m.blurb}</span>
              </span>
              {#if m.id === mode}
                <span class="mode-check" aria-hidden="true">✓</span>
              {:else if !m.ready}
                <span class="soon">Soon</span>
              {/if}
            </button>
          {/each}
          {#if mode === "math"}
            <div class="menu-section">Math workspaces</div>
            {#each WORKSPACES as w}
              <button
                class="mode-item"
                class:active={w.id === workspace}
                role="menuitemradio"
                aria-checked={w.id === workspace}
                aria-disabled={!w.ready}
                disabled={!w.ready}
                onclick={() => chooseWorkspace(w.id)}
              >
                <span class="mode-icon">{w.icon}</span>
                <span class="mode-text">
                  <span class="mode-name">{w.name}</span>
                  <span class="mode-blurb">{w.blurb}</span>
                </span>
                {#if w.id === workspace}
                  <span class="mode-check" aria-hidden="true">✓</span>
                {:else if !w.ready}
                  <span class="soon">Soon</span>
                {/if}
              </button>
            {/each}
          {/if}
        </div>
      {/if}
    </div>
    <div class="file-wrap">
      <button class="ghost file-button" aria-haspopup="menu" aria-expanded={fileOpen} onclick={() => (fileOpen = !fileOpen)} title={projectPath ?? "Not saved yet"}>
        <span class="project-name">{projectName(projectPath)}</span>
        {#if dirty}<span class="dirty" title="Unsaved changes" aria-label="Unsaved changes">●</span>{/if}
        <span class="caret" class:up={fileOpen}>▾</span>
      </button>
      {#if fileOpen}
        <div class="menu-backdrop" role="presentation" onclick={() => (fileOpen = false)}></div>
        <div class="file-menu" role="menu" aria-label="File">
          <button class="file-item" role="menuitem" onclick={newProject}><span>New project</span><kbd>Ctrl+N</kbd></button>
          <button class="file-item" role="menuitem" onclick={() => openProject()}><span>Open…</span><kbd>Ctrl+O</kbd></button>
          <button class="file-item" role="menuitem" onclick={() => saveProject(false)}><span>Save</span><kbd>Ctrl+S</kbd></button>
          <button class="file-item" role="menuitem" onclick={() => saveProject(true)}><span>Save as…</span><kbd>Ctrl+Shift+S</kbd></button>
          {#if recent.length}
            <div class="file-sep">Recent</div>
            {#each recent as path}
              <button class="file-item recent" role="menuitem" title={path} onclick={() => openProject(path)}>
                <span>{projectName(path)}</span><span class="recent-ext">{path.split(".").pop()}</span>
              </button>
            {/each}
          {/if}
          {#if projectPath}
            <div class="file-note">Saving automatically to {projectName(projectPath)}</div>
          {/if}
        </div>
      {/if}
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
      <button
        class="ghost"
        class:on={historyOpen}
        onclick={() => (historyOpen = !historyOpen)}
        title="Results you pinned with Ctrl+Enter"
      >
        History{#if history.length}<span class="count" class:pulse={justPinned}>{history.length}</span>{/if}
      </button>
      <button class="info" onclick={() => settingsDialog?.showModal()} title="Settings" aria-label="Settings">⚙</button>
      <button class="info" onclick={() => tipsDialog?.showModal()} title="What you can write" aria-label="Tips">ⓘ</button>
    </div>
  </header>

  <dialog bind:this={settingsDialog} class="tips" onclick={(e) => e.target === settingsDialog && settingsDialog?.close()}>
    <div class="tips-body">
      <div class="tips-head">
        <h2>Settings</h2>
        <button class="ghost" onclick={() => settingsDialog?.close()} aria-label="Close">✕</button>
      </div>
      <div class="setting">
        <div class="setting-text">
          <span class="setting-name">Theme</span>
          <span class="setting-help">Light, dark, or follow your system</span>
        </div>
        <div class="seg" role="group" aria-label="Theme">
          {#each THEMES as t}
            <button class:on={theme.choice === t.id} onclick={() => theme.set(t.id)}>{t.name}</button>
          {/each}
        </div>
      </div>
      <div class="setting">
        <div class="setting-text">
          <span class="setting-name">Angles</span>
          <span class="setting-help">How sin, cos and tan read their angles, and how angles are shown</span>
        </div>
        <div class="seg" role="group" aria-label="Angle unit">
          <button class:on={settings.angle === "degrees"} onclick={() => setAngle("degrees")}>DEG</button>
          <button class:on={settings.angle === "radians"} onclick={() => setAngle("radians")}>RAD</button>
        </div>
      </div>
      <div class="setting">
        <div class="setting-text">
          <span class="setting-name">Digits</span>
          <span class="setting-help">Significant digits in decimal answers</span>
        </div>
        <select value={settings.digits} onchange={setDigits}>
          {#each DIGIT_CHOICES as d}
            <option value={d}>{d}</option>
          {/each}
        </select>
      </div>
      <div class="setting">
        <div class="setting-text">
          <span class="setting-name">Length unit</span>
          <span class="setting-help">Used for geometry and measurements; areas and volumes follow (m² and m³ for metres)</span>
        </div>
        <select value={lengthUnit} onchange={setLengthUnit}>
          {#each LENGTH_UNITS as u}
            <option value={u.id}>{u.name}</option>
          {/each}
        </select>
      </div>

      <h3 class="setting-group">Updates</h3>
      <div class="setting">
        <div class="setting-text">
          <span class="setting-name">ProCalc {updates.version || ""}</span>
          <span class="setting-help update-status" class:err={updates.status.kind === "error"} role="status">
            {#if updates.status.kind === "checking"}
              Checking for updates…
            {:else if updates.status.kind === "current"}
              ✓ You have the latest version
            {:else if updates.status.kind === "available" && updates.update}
              Version {updates.update.version} is available
            {:else if updates.status.kind === "downloading"}
              Downloading update… {formatBytes(updates.status.done)}
            {:else if updates.status.kind === "restarting"}
              Restarting…
            {:else if updates.status.kind === "error"}
              {updates.status.message}
            {:else if updates.lastChecked}
              Last checked {lastCheckedText(updates.lastChecked)}
            {:else}
              Not checked yet
            {/if}
          </span>
        </div>
        {#if updates.status.kind === "available" && updates.update}
          <button class="ghost" onclick={() => (updates.prompt = true)}>{updates.portable ? "Download…" : "Install…"}</button>
        {:else}
          <button
            class="ghost"
            disabled={updates.status.kind === "checking" || updates.status.kind === "downloading" || !updates.version}
            onclick={() => updates.checkNow(true)}
          >Check now</button>
        {/if}
      </div>
      <div class="setting">
        <div class="setting-text">
          <span class="setting-name">Check automatically</span>
          <span class="setting-help">
            {#if updates.portable}
              Looks for a new version when ProCalc starts. This portable copy can't install updates itself, so you'll get a download link.
            {:else}
              Looks for a new version when ProCalc starts and offers to install it. Nothing is installed without asking.
            {/if}
          </span>
        </div>
        <label class="switch">
          <input type="checkbox" checked={updates.auto} onchange={(e) => updates.setAuto((e.target as HTMLInputElement).checked)} />
          <span class="track" aria-hidden="true"></span>
          <span class="sr-only">Check for updates automatically</span>
        </label>
      </div>
    </div>
  </dialog>

  <UpdateDialog beforeInstall={beforeUpdate} />

  {#if toast}
    <div class="toast" role="status">{toast}</div>
  {/if}

  <dialog bind:this={tipsDialog} class="tips" onclick={(e) => e.target === tipsDialog && tipsDialog?.close()}>
    <div class="tips-body">
      <div class="tips-head">
        <h2>What you can write</h2>
        <button class="ghost" onclick={() => tipsDialog?.close()} aria-label="Close">✕</button>
      </div>
      <ul>
        <li><code>sqrt</code> → √, <code>/</code> → fraction, <code>^</code> → power, <code>pi</code> → π</li>
        <li>Every Paper line answers as you type; <kbd>Enter</kbd> adds a line, <kbd>Ctrl</kbd>+<kbd>Enter</kbd> pins it to the history.</li>
        <li>Exact when possible (0.1 + 0.2 = 3/10), up to 1000 digits otherwise.</li>
        <li>
          Algebra: <code>x+x</code>, <code>factor(x^2-1)</code>, <code>expand((x+1)^3)</code>, <code>(x^2-1)/(x-1)</code> = x+1,
          <code>simplify(1/x + 1/(x+1))</code>, <code>apart(1/(x^2-1))</code>, <code>divide(x^3+1, x-1)</code>
        </li>
        <li>Definitions for the lines below: <code>x := 5</code>, <code>f(x) := x^2</code>, then <code>f(x+1)</code></li>
        <li>Equations and systems: <code>2x+3=7</code>, <code>x+y=3, x-y=1</code>, with conditions: <code>sin x = 1/2, 0 ≤ x ≤ 2π</code></li>
        <li>Inequalities: <code>x^2 &lt; 4</code> → −2 &lt; x &lt; 2</li>
        <li>Calculus: <code>d/dx x sin x</code>, <code>f'(2)</code>, <code>∫ x e^x dx</code>, <code>∫_0^1 e^(-x^2) dx</code>, <code>(2x+1)|_(x=3)</code></li>
        <li>
          Limits and series: <code>lim_(x→0) sin x / x</code>, <code>lim_(x→∞) (1+1/x)^x</code>, <code>∑_(k=1)^n k^2</code>,
          <code>∑_(k=1)^∞ 1/k^2</code>, <code>∫_0^∞ e^(-x) dx</code>, <code>taylor(sin x, 0, 7)</code>
        </li>
        <li>
          Complex numbers: <code>sqrt(-4)</code> = 2i, <code>1/(1+i)</code>, <code>Re</code>, <code>Im</code>, <code>arg</code>,
          <code>conj</code>, <code>polar(1+i)</code>, <code>2∠60°</code>, <code>roots(8, 3)</code>
        </li>
        <li>
          Optimisation: <code>minimize(x^2-4x+7)</code>, <code>maximize(x^3-3x, -3, 3)</code>, <code>extrema(x^3-3xy+y^3)</code>,
          <code>maximize(xy, x+y=10)</code>
        </li>
        <li>
          Geometry: <code>triangle(a=3, b=4, C=60°)</code>, <code>circle(r=2)</code>, <code>cone(r=3, h=4)</code>,
          <code>distance([1,2],[4,6])</code>, <code>line([1,2],[3,8])</code>, <code>polygon([0,0],[4,0],[4,3])</code>
        </li>
        <li>
          Statistics: <code>L := [2,4,4,5,7]</code>, then <code>mean(L)</code>, <code>median(L)</code>, <code>sd(L)</code> (sample),
          <code>psd(L)</code>, <code>quartile(L, 1)</code>, <code>stats(L)</code>, <code>linreg(xs, ys)</code>; probability:
          <code>binompdf(10, 0.5, 3)</code>, <code>normalcdf(-1.96, 1.96)</code>, <code>invnorm(0.975)</code>
        </li>
        <li>
          Differential equations: <code>y' = 2y</code>, <code>y'+y=x, y(0)=1</code>, <code>y''+y=0, y(0)=0, y'(0)=1</code>,
          <code>dy/dx = x y</code>; solutions draw on the Graph
        </li>
        <li>
          Matrices (insert one from the keyboard, or <code>\begin&#123;pmatrix&#125;1&amp;2\\3&amp;4\end&#123;pmatrix&#125;</code>):
          <code>A := …</code>, then <code>A B</code>, <code>A^-1</code>, <code>A^T</code>, <code>det(A)</code>, <code>rref(A)</code>,
          <code>eigenvalues(A)</code>, <code>eigenvectors(A)</code>; vectors <code>[1,2,3]</code>, <code>u·v</code>, <code>u×v</code>,
          <code>|v|</code>
        </li>
        <li>
          Number theory: <code>gcd(12, 18)</code>, <code>lcm(4, 6)</code>, <code>17 mod 5</code>, <code>nCr(5, 2)</code>,
          <code>factor(360)</code>, <code>isprime(91)</code>, <code>divisors(12)</code>, <code>Γ(1/2)</code>, <code>2.5!</code>
        </li>
        <li>
          Units: <code>60 mph -&gt; km/h</code>, <code>20°C -&gt; °F</code>; kg, km… turn into units as you type, others go in
          quotes: <code>5 "m"</code>
        </li>
        <li>
          <b>Graph</b> plots <code>x^2</code>, <code>y = 2x+1</code>, <code>x^2+y^2=1</code>, <code>y &gt; x^2</code>,
          <code>r = 1+cos θ</code>, <code>x=cos t, y=sin t, 0 ≤ t ≤ 4π</code>, <code>z = x^2-y^2</code> (3D),
          <code>∫_0^2 x^2 dx</code> (shaded) and <code>f'(1)</code> (tangent).
        </li>
        <li><b>Steps</b> shows the working for equations, derivatives, integrals, limits, sums and more, with a choice of methods.</li>
        <li>The number at the start of a line switches it to a text note (¶) and back.</li>
        <li>
          <b>Geometry</b> (ProCalc menu → Math workspaces): draw a triangle, rectangle, polygon or circle, click a side or
          angle to give its value, and the figure redraws to scale with its area, perimeter and more in the Inspector.
        </li>
        <li>
          <b>Unit circle</b> (ProCalc menu → Math workspaces): drag the point round the circle, or type an angle, to see
          sin, cos and tan as lengths, their exact values at the standard angles, and the matching points on the sine and
          cosine graphs.
        </li>
      </ul>
    </div>
  </dialog>

  <div class="page">
    {#if ready}
      <!-- Both stay mounted so switching keeps each one's state. -->
      <div class="workspace" class:hidden={workspace !== "paper"}>
        <Paper bind:this={paper} {settings} {render} onpin={pinned} onchange={projectChanged} />
      </div>
      <div class="workspace" class:hidden={workspace !== "geometry"}>
        <Geometry bind:this={geometry} {settings} {render} {lengthUnit} onchange={projectChanged} />
      </div>
      <div class="workspace" class:hidden={workspace !== "unit-circle"}>
        <UnitCircle bind:this={unitCircle} {settings} {render} onchange={projectChanged} />
      </div>
    {/if}
  </div>

  <aside class="drawer" class:open={historyOpen} aria-label="History" aria-hidden={!historyOpen}>
    <div class="drawer-head">
      <span class="drawer-title">History</span>
      <span class="spacer"></span>
      {#if history.length}
        <button class="ghost small" onclick={clearHistory} title="Clear the history">Clear</button>
      {/if}
      <button class="ghost small" onclick={() => (historyOpen = false)} aria-label="Close the history">✕</button>
    </div>
  <div class="feed" bind:this={feed}>
    {#if history.length === 0}
      <p class="empty">Pinned results land here: press <kbd>Ctrl</kbd>+<kbd>Enter</kbd> on a Paper line, or its 📌.</p>
    {/if}

    {#if ready}
      {#each history as entry (entry.id)}
        <article class="entry" class:definition={entry.answer?.defines}>
          <div class="input">{@html render(entry.input)}</div>
          {#if entry.answer}
            {@const a = entry.answer}
            <div class="result">
              {#if a.exact_latex}
                <button class="value exact" onclick={() => reuse(a.exact_latex!)} title="Insert into the Paper">
                  <span class="rel">=</span>{@html render(a.exact_latex)}
                </button>
              {/if}
              {#if showDecimal(a)}
                <button class="value decimal" onclick={() => reuse(a.decimal!.latex)} title="Insert into the Paper">
                  <span class="rel">{rel(a)}</span>{@html render(a.decimal!.latex)}
                </button>
              {/if}
            </div>
            <div class="actions">
              <button class="ghost small" onclick={() => copy(entry)}>
                {copied === entry.id ? "Copied" : "Copy"}
              </button>
              {#if a.defines}
                <span class="tag">defines {@html render(a.defines)}</span>
              {:else if a.symbolic}
                <button class="ghost small" onclick={() => applyOp(entry, "expand")}>Expand</button>
                <button class="ghost small" onclick={() => applyOp(entry, "factor")}>Factor</button>
              {/if}
            </div>
          {/if}
        </article>
      {/each}
    {/if}
    {#if notice}
      <div class="notice">{notice}</div>
    {/if}
  </div>
  </aside>
</div>

<style>
  /* Header, then the Paper filling the rest; the history slides over from the right. */
  .app {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr);
    height: 100vh;
    height: 100dvh;
    position: relative;
    overflow: hidden;
  }

  .page {
    min-height: 0;
    padding-top: 6px;
  }

  .workspace {
    height: 100%;
    min-height: 0;
  }

  .workspace.hidden {
    display: none;
  }

  .menu-section {
    margin: 8px 10px 2px;
    padding-top: 8px;
    border-top: 1px solid var(--line);
    color: var(--muted);
    font-size: 11px;
    letter-spacing: 0.4px;
    text-transform: uppercase;
  }

  .drawer {
    position: absolute;
    top: 55px;
    right: 0;
    bottom: 0;
    width: min(440px, 100vw);
    display: flex;
    flex-direction: column;
    background: var(--panel);
    border-radius: var(--radius-lg) 0 0 var(--radius-lg);
    box-shadow: var(--shadow);
    transform: translateX(105%);
    transition: transform 180ms ease;
    z-index: 5;
  }

  .drawer.open {
    transform: none;
  }

  .drawer-head {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px 14px 8px;
  }

  .drawer-title {
    font-weight: 650;
    font-size: 13px;
    letter-spacing: 0.4px;
    text-transform: uppercase;
    color: var(--muted);
  }

  .spacer {
    flex: 1;
  }

  .count {
    margin-left: 6px;
    padding: 0 6px;
    border-radius: 999px;
    background: var(--panel-2);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }

  .count.pulse {
    background: var(--accent);
    color: var(--accent-ink);
  }

  .ghost.on {
    background: var(--accent);
    color: var(--accent-ink);
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 14px 20px 8px;
  }

  .brand-wrap,
  .file-wrap {
    position: relative;
  }

  .file-wrap {
    margin-right: auto;
  }

  .file-button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    max-width: 280px;
  }

  .project-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .dirty {
    color: var(--accent-strong);
    font-size: 9px;
  }

  .file-menu {
    position: absolute;
    top: calc(100% + 8px);
    left: 0;
    z-index: 10;
    width: 280px;
    padding: 8px;
    border-radius: var(--radius);
    background: var(--panel);
    box-shadow: var(--shadow);
    animation: menu-in 140ms ease-out;
  }

  .file-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    width: 100%;
    padding: 8px 10px;
    border: 0;
    border-radius: 9px;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .file-item:hover {
    background: var(--panel-2);
  }

  .file-item span:first-child {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .file-item kbd,
  .recent-ext {
    color: var(--muted);
    font-family: inherit;
    font-size: 11px;
  }

  .file-sep {
    margin: 6px 10px 2px;
    padding-top: 8px;
    border-top: 1px solid var(--line);
    color: var(--muted);
    font-size: 11px;
    letter-spacing: 0.4px;
    text-transform: uppercase;
  }

  .file-note {
    margin: 6px 10px 2px;
    color: var(--muted);
    font-size: 11.5px;
  }

  .setting {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 12px 0;
    border-top: 1px solid var(--line);
  }

  .setting-text {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .setting-name {
    font-weight: 600;
  }

  .setting-help {
    color: var(--muted);
    font-size: 12.5px;
  }

  .setting-group {
    margin: 14px 0 0;
    padding-top: 12px;
    border-top: 1px solid var(--line);
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
  }

  .setting-group + .setting {
    border-top: none;
  }

  .update-status.err {
    color: var(--err);
  }

  .setting .ghost:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .switch {
    position: relative;
    flex: none;
    cursor: pointer;
  }

  .switch input {
    position: absolute;
    opacity: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    cursor: pointer;
  }

  .switch .track {
    display: block;
    width: 38px;
    height: 22px;
    border-radius: 11px;
    background: var(--line);
    transition: background 150ms;
  }

  .switch .track::after {
    content: "";
    position: absolute;
    top: 3px;
    left: 3px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--ink);
    transition: transform 150ms;
  }

  .switch input:checked + .track {
    background: var(--accent);
  }

  .switch input:checked + .track::after {
    transform: translateX(16px);
    background: var(--accent-ink);
  }

  .switch input:focus-visible + .track {
    outline: 2px solid var(--accent-strong);
    outline-offset: 2px;
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }

  .toast {
    position: fixed;
    bottom: 22px;
    left: 50%;
    z-index: 20;
    max-width: min(520px, 90vw);
    padding: 12px 20px;
    border-radius: 999px;
    background: var(--ink);
    color: var(--bg);
    font-weight: 600;
    box-shadow: var(--shadow);
    transform: translateX(-50%);
    animation: menu-in 160ms ease-out;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 5px 12px 5px 5px;
    border: none;
    border-radius: 999px;
    background: transparent;
    color: inherit;
    font: inherit;
    font-size: 17px;
    font-weight: 700;
    letter-spacing: -0.2px;
    cursor: pointer;
    transition: background 120ms ease;
  }

  .brand:hover,
  .brand[aria-expanded="true"] {
    background: var(--panel);
  }

  .mode-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 10px 4px 8px;
    border-radius: 999px;
    background: var(--panel);
    color: var(--ink);
    font-size: 13px;
    font-weight: 600;
  }

  .mode-chip::before {
    content: "";
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--accent);
  }

  .caret {
    font-size: 10px;
    transition: transform 150ms ease;
  }

  .caret.up {
    transform: rotate(180deg);
  }

  .menu-backdrop {
    position: fixed;
    inset: 0;
    z-index: 9;
  }

  .mode-menu {
    position: absolute;
    top: calc(100% + 8px);
    left: 0;
    z-index: 10;
    width: 300px;
    padding: 8px;
    border-radius: var(--radius);
    background: var(--panel);
    box-shadow: var(--shadow);
    animation: menu-in 140ms ease-out;
    transform-origin: top left;
  }

  @keyframes menu-in {
    from {
      opacity: 0;
      transform: translateY(-4px) scale(0.98);
    }
  }

  .mode-item {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 9px 10px;
    border: 0;
    border-radius: 14px;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .mode-item:hover:not(:disabled) {
    background: var(--panel-2);
  }

  .mode-item:disabled {
    cursor: default;
    opacity: 0.55;
  }

  .mode-icon {
    display: grid;
    place-items: center;
    flex: none;
    width: 36px;
    height: 36px;
    border-radius: 50%;
    background: var(--panel-2);
    font-size: 17px;
  }

  .mode-item.active .mode-icon {
    background: var(--accent);
    color: var(--accent-ink);
  }

  .mode-text {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }

  .mode-name {
    font-weight: 600;
  }

  .mode-blurb {
    color: var(--muted);
    font-size: 12px;
  }

  .mode-check {
    color: var(--accent-strong);
    font-weight: 700;
  }

  .soon {
    padding: 3px 8px;
    border-radius: 999px;
    background: var(--panel-2);
    color: var(--muted);
    font-size: 10.5px;
    letter-spacing: 0.3px;
    text-transform: uppercase;
  }

  .logo {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    border-radius: 50%;
    background: var(--accent);
    color: var(--accent-ink);
    font-size: 18px;
  }

  .controls {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  /* Pill switch, like Month | Year. */
  .seg {
    display: flex;
    background: var(--panel);
    border-radius: 999px;
    padding: 4px;
    gap: 2px;
  }

  .tips .seg {
    background: var(--panel-2);
  }

  .seg button {
    border: none;
    background: none;
    padding: 5px 13px;
    border-radius: 999px;
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0.3px;
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

  .digits {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 4px 4px 14px;
    border-radius: 999px;
    background: var(--panel);
    font-size: 13px;
    font-weight: 600;
    color: var(--muted);
  }

  .digits select,
  .setting select {
    background: var(--panel-2);
    color: var(--ink);
    border: none;
    border-radius: 999px;
    padding: 5px 10px;
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }

  .ghost {
    background: var(--panel);
    border: none;
    border-radius: 999px;
    padding: 7px 14px;
    font-size: 13px;
    font-weight: 600;
    color: var(--muted);
    cursor: pointer;
    transition: background 140ms, color 140ms;
  }

  .ghost:hover {
    color: var(--ink);
    background: var(--panel-2);
  }

  .tips .ghost,
  .drawer .ghost {
    background: var(--panel-2);
  }

  .ghost.small {
    padding: 2px 8px;
    font-size: 12px;
  }

  .feed {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 4px 14px 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .empty {
    margin: auto 0;
    text-align: center;
    color: var(--muted);
    font-size: 14px;
  }

  .info {
    width: 36px;
    height: 36px;
    border-radius: 50%;
    border: none;
    background: var(--panel);
    color: var(--muted);
    font-size: 16px;
    cursor: pointer;
    transition: background 140ms, color 140ms;
  }

  .info:hover {
    color: var(--ink);
    background: var(--panel-2);
  }

  .tips {
    border: none;
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow);
    background: var(--panel);
    color: var(--ink);
    padding: 0;
    width: min(640px, calc(100vw - 32px));
    max-height: calc(100dvh - 48px);
  }

  .tips::backdrop {
    background: rgb(0 0 0 / 0.45);
  }

  .tips-body {
    padding: 20px 24px 22px;
    line-height: 1.7;
    color: var(--muted);
  }

  .tips-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 4px;
  }

  .tips h2 {
    margin: 0;
    font-size: 17px;
    color: var(--ink);
  }

  .tips ul {
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

  .entry.definition {
    border-left: 3px solid var(--accent-strong);
  }

  .tag {
    display: flex;
    align-items: baseline;
    gap: 4px;
    font-size: 12px;
    color: var(--muted);
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
    color: var(--accent-strong);
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

  .notice {
    position: sticky;
    bottom: 0;
    align-self: center;
    background: var(--panel-2);
    border: 1px solid var(--err);
    color: var(--err);
    border-radius: 10px;
    padding: 6px 12px;
    font-size: 14px;
  }

  @media (max-width: 560px) {
    .digits span {
      display: none;
    }
    .value.exact {
      font-size: 22px;
    }
  }
</style>
