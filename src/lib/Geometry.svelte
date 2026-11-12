<script lang="ts">
  // The geometry workspace: draw a shape, give the sides and angles you know, and the figure
  // adjusts itself to scale. The Inspector lists every part (given, found or unknown) and
  // offers the quantities (area, perimeter, …) with the formula and the working.
  import { onMount } from "svelte";
  import { solveFigure, type FigureInput, type FigureKind, type FigureResult, type Settings } from "$lib/engine";

  let {
    settings,
    render,
    lengthUnit,
    onchange,
  }: {
    settings: Settings;
    render: (latex: string) => string;
    lengthUnit: string;
    /** The figure changed (worth saving). */
    onchange?: () => void;
  } = $props();

  type Tool = "select" | FigureKind;
  type Pt = [number, number];

  const STORE_KEY = "procalc.geometry.v1";
  const TOOLS: { id: Tool; icon: string; label: string; hint: string }[] = [
    { id: "select", icon: "⬉", label: "Select", hint: "Move corners; click a side or angle to give its value" },
    { id: "triangle", icon: "△", label: "Triangle", hint: "Click three corners" },
    { id: "rectangle", icon: "▭", label: "Rectangle", hint: "Drag from one corner to the opposite one" },
    { id: "square", icon: "□", label: "Square", hint: "Drag from one corner to the opposite one" },
    { id: "polygon", icon: "⬠", label: "Polygon", hint: "Click the corners; click the first one again (or press Enter) to close" },
    { id: "circle", icon: "○", label: "Circle", hint: "Press at the centre and drag out the radius" },
  ];
  /** Metres per unit, for the area tooltips. */
  const METRES: Record<string, number> = { mm: 0.001, cm: 0.01, m: 1, km: 1000, in: 0.0254, ft: 0.3048, yd: 0.9144, mi: 1609.344 };

  let tool = $state<Tool>("triangle");
  let figure = $state<FigureInput | null>(null);
  let result = $state<FigureResult | null>(null);
  /** Corners clicked so far for a triangle or polygon. */
  let draft = $state<Pt[]>([]);
  /** A rectangle, square or circle being dragged out: its start and the current point. */
  let stretch = $state<{ from: Pt; to: Pt } | null>(null);
  let hover = $state<Pt | null>(null);
  let dragging: number | null = null;
  /** The view stays put while something is being dragged. */
  let frozen: View | null = null;
  let box = $state<HTMLElement | undefined>();
  let size = $state({ w: 800, h: 500 });
  let editing = $state<{ type: "side" | "angle" | "circle"; index: number; x: number; y: number; value: string; which: "r" | "d" | "C" | "A" } | null>(null);
  let openCards = $state<Record<string, boolean>>({ area: true, perimeter: true });
  let solveTimer: ReturnType<typeof setTimeout> | undefined;
  let solveSeq = 0;

  interface View {
    x0: number;
    y0: number;
    k: number;
  }

  // ---------- State and saving ----------

  export function getState(): FigureInput | null {
    return figure ? JSON.parse(JSON.stringify(figure)) : null;
  }

  export function setState(f: FigureInput | null) {
    figure = f ? JSON.parse(JSON.stringify(f)) : null;
    draft = [];
    editing = null;
    tool = figure ? "select" : "triangle";
    changed();
  }

  function changed() {
    try {
      localStorage.setItem(STORE_KEY, JSON.stringify(figure));
    } catch {
      /* storage unavailable: the figure just won't persist */
    }
    onchange?.();
    clearTimeout(solveTimer);
    solveTimer = setTimeout(solve, 120);
  }

  async function solve() {
    if (!figure) {
      result = null;
      return;
    }
    const seq = ++solveSeq;
    const r = await solveFigure($state.snapshot(figure) as FigureInput, $state.snapshot(settings) as Settings);
    if (seq === solveSeq) result = r;
  }

  onMount(() => {
    try {
      const saved = JSON.parse(localStorage.getItem(STORE_KEY) ?? "null");
      if (saved && Array.isArray(saved.points)) {
        figure = saved;
        tool = "select";
      }
    } catch {
      /* first run */
    }
    solve();
    const ro = new ResizeObserver(() => {
      // A hidden workspace measures 0: keep the last real size.
      if (box && box.clientWidth > 0 && box.clientHeight > 0) size = { w: box.clientWidth, h: box.clientHeight };
    });
    if (box) ro.observe(box);
    return () => ro.disconnect();
  });

  $effect(() => {
    void settings.digits;
    solve();
  });

  // ---------- Geometry helpers ----------

  const solved = $derived(result?.status === "solved");
  /** The corners as shown: to scale once solved, else as drawn. */
  const shown = $derived<Pt[]>(figure ? (solved && result ? (result.points as Pt[]) : (figure.points as Pt[])) : []);
  const shownRadius = $derived(
    figure?.kind === "circle"
      ? solved && result?.radius
        ? result.radius
        : figure.points.length > 1
          ? Math.hypot(figure.points[1][0] - figure.points[0][0], figure.points[1][1] - figure.points[0][1])
          : 1
      : 0,
  );

  function corner(i: number) {
    return String.fromCharCode(65 + (i % 26)) + (i >= 26 ? Math.floor(i / 26) : "");
  }

  function sideName(i: number, n: number) {
    return corner(i) + corner((i + 1) % n);
  }

  /** A short number for labels on the drawing. */
  function short(v: number | null | undefined) {
    if (v == null || !Number.isFinite(v)) return "?";
    const s = Math.abs(v) >= 1e6 || (Math.abs(v) < 1e-3 && v !== 0) ? v.toExponential(3) : String(Number(v.toPrecision(6)));
    return s;
  }

  const unitText = $derived(lengthUnit);

  function withUnit(v: string, dim: number) {
    if (dim === 1) return `${v} ${unitText}`;
    if (dim === 2) return `${v} ${unitText}²`;
    return v;
  }

  function unitLatex(dim: number) {
    if (dim === 1) return `\\,\\mathrm{${lengthUnit}}`;
    if (dim === 2) return `\\,\\mathrm{${lengthUnit}}^{2}`;
    return "";
  }

  /** Extra ways to read an area: m², hectares, acres, ft². */
  function areaTip(value: number) {
    const f = METRES[lengthUnit];
    if (!f) return "";
    const m2 = value * f * f;
    const fmt = (x: number) => Number(x.toPrecision(6)).toLocaleString();
    return `= ${fmt(m2)} m² = ${fmt(m2 / 10000)} ha = ${fmt(m2 / 4046.8564224)} acres = ${fmt(m2 / 0.09290304)} ft²`;
  }

  // ---------- View ----------

  function niceStep(span: number) {
    const raw = span / 12;
    const p = Math.pow(10, Math.floor(Math.log10(raw)));
    const m = raw / p;
    return (m < 1.5 ? 1 : m < 3.5 ? 2 : m < 7.5 ? 5 : 10) * p;
  }

  const view = $derived.by<View>(() => {
    if (frozen) return frozen;
    const pts: Pt[] = [...shown, ...draft];
    if (figure?.kind === "circle" && shown.length) {
      const [cx, cy] = shown[0];
      pts.push([cx - shownRadius, cy - shownRadius], [cx + shownRadius, cy + shownRadius]);
    }
    let [x0, x1, y0, y1] = [0, 24, 0, 14];
    if (pts.length >= 2) {
      x0 = Math.min(...pts.map((p) => p[0]));
      x1 = Math.max(...pts.map((p) => p[0]));
      y0 = Math.min(...pts.map((p) => p[1]));
      y1 = Math.max(...pts.map((p) => p[1]));
      const span = Math.max(x1 - x0, y1 - y0, 1e-9);
      const pad = span * 0.35;
      x0 -= pad;
      x1 += pad;
      y0 -= pad;
      y1 += pad;
    }
    const k = Math.min(size.w / (x1 - x0), size.h / (y1 - y0));
    // Centre the box in the sheet.
    const cx = (x0 + x1) / 2;
    const cy = (y0 + y1) / 2;
    return { x0: cx - size.w / 2 / k, y0: cy - size.h / 2 / k, k };
  });

  const step = $derived(niceStep(size.w / view.k));

  function toScreen(p: Pt): Pt {
    return [(p[0] - view.x0) * view.k, size.h - (p[1] - view.y0) * view.k];
  }

  function toWorld(e: PointerEvent | MouseEvent): Pt {
    const r = box!.getBoundingClientRect();
    return [(e.clientX - r.left) / view.k + view.x0, (size.h - (e.clientY - r.top)) / view.k + view.y0];
  }

  function snap(p: Pt, e: PointerEvent | MouseEvent): Pt {
    if (e.altKey) return p;
    const g = step / 2;
    return [Math.round(p[0] / g) * g, Math.round(p[1] / g) * g];
  }

  const gridLines = $derived.by(() => {
    const xs: number[] = [];
    const ys: number[] = [];
    const x1 = view.x0 + size.w / view.k;
    const y1 = view.y0 + size.h / view.k;
    for (let x = Math.ceil(view.x0 / step) * step; x <= x1; x += step) xs.push(x);
    for (let y = Math.ceil(view.y0 / step) * step; y <= y1; y += step) ys.push(y);
    return { xs, ys };
  });

  // ---------- Drawing ----------

  function start(kind: FigureKind, points: Pt[]) {
    const n = kind === "circle" ? 0 : points.length;
    figure = { kind, points, sides: Array(n).fill(null), angles: Array(n).fill(null), circle: null };
    draft = [];
    tool = "select";
    editing = null;
    changed();
  }

  function nearCorner(e: PointerEvent): number | null {
    if (!figure) return null;
    const r = box!.getBoundingClientRect();
    const m: Pt = [e.clientX - r.left, e.clientY - r.top];
    for (let i = 0; i < shown.length; i++) {
      const s = toScreen(shown[i]);
      if (Math.hypot(s[0] - m[0], s[1] - m[1]) < 12) return i;
    }
    if (figure.kind === "circle" && shown.length) {
      const rim = toScreen([shown[0][0] + shownRadius, shown[0][1]]);
      if (Math.hypot(rim[0] - m[0], rim[1] - m[1]) < 12) return 1;
    }
    return null;
  }

  function pointerDown(e: PointerEvent) {
    if (e.button !== 0) return;
    editing = null;
    const p = snap(toWorld(e), e);
    if (tool === "select") {
      const i = nearCorner(e);
      if (i == null || !figure) return;
      // Dragging starts from what's on screen.
      if (figure.kind === "circle") {
        figure.points = [shown[0], [shown[0][0] + shownRadius, shown[0][1]]];
      } else {
        figure.points = shown.map((q) => [q[0], q[1]] as Pt);
      }
      dragging = i;
      frozen = view;
      box!.setPointerCapture(e.pointerId);
      return;
    }
    if (tool === "triangle" || tool === "polygon") {
      if (tool === "polygon" && draft.length >= 3) {
        const first = toScreen(draft[0]);
        const here = toScreen(p);
        if (Math.hypot(first[0] - here[0], first[1] - here[1]) < 12) {
          start("polygon", draft);
          return;
        }
      }
      draft = [...draft, p];
      if (tool === "triangle" && draft.length === 3) start("triangle", draft);
      return;
    }
    stretch = { from: p, to: p };
    frozen = view;
    box!.setPointerCapture(e.pointerId);
  }

  function pointerMove(e: PointerEvent) {
    const p = snap(toWorld(e), e);
    hover = p;
    if (dragging != null && figure) {
      figure.points[dragging] = p;
      changed();
    } else if (stretch) {
      stretch = { from: stretch.from, to: p };
    }
  }

  function boxCorners(a: Pt, b: Pt, square: boolean): Pt[] {
    let [w, h] = [b[0] - a[0], b[1] - a[1]];
    if (square) {
      const s = Math.max(Math.abs(w), Math.abs(h));
      w = Math.sign(w || 1) * s;
      h = Math.sign(h || 1) * s;
    }
    const [x0, x1] = [Math.min(a[0], a[0] + w), Math.max(a[0], a[0] + w)];
    const [y0, y1] = [Math.min(a[1], a[1] + h), Math.max(a[1], a[1] + h)];
    // Anticlockwise from the bottom left.
    return [
      [x0, y0],
      [x1, y0],
      [x1, y1],
      [x0, y1],
    ];
  }

  function pointerUp() {
    if (dragging != null) {
      dragging = null;
      frozen = null;
      return;
    }
    if (!stretch) return;
    const { from, to } = stretch;
    stretch = null;
    frozen = null;
    if (Math.hypot(to[0] - from[0], to[1] - from[1]) < 1e-9) return;
    if (tool === "circle") start("circle", [from, to]);
    else if (tool === "rectangle" || tool === "square") start(tool, boxCorners(from, to, tool === "square"));
  }

  function keydown(e: KeyboardEvent) {
    if (editing) return;
    if (e.key === "Escape") draft = [];
    if (e.key === "Enter" && tool === "polygon" && draft.length >= 3) start("polygon", draft);
  }

  function clearFigure() {
    figure = null;
    result = null;
    draft = [];
    editing = null;
    tool = "triangle";
    changed();
  }

  // ---------- Labels ----------

  const centroid = $derived<Pt>(
    shown.length ? [shown.reduce((s, p) => s + p[0], 0) / shown.length, shown.reduce((s, p) => s + p[1], 0) / shown.length] : [0, 0],
  );

  /** A screen point pushed `by` pixels away from (or, negative, toward) the middle of the figure. */
  function outward(p: Pt, by: number): Pt {
    const s = toScreen(p);
    const c = toScreen(centroid);
    const d = Math.hypot(s[0] - c[0], s[1] - c[1]) || 1;
    return [s[0] + ((s[0] - c[0]) / d) * by, s[1] + ((s[1] - c[1]) / d) * by];
  }

  function sideInfo(i: number) {
    const part = result?.sides?.[i];
    const typed = figure?.sides[i];
    if (part && part.status !== "unknown") return { status: part.status, text: short(part.value), part };
    if (typed) return { status: "given", text: typed, part: null };
    return { status: "unknown", text: "?", part: null };
  }

  function angleInfo(i: number) {
    const part = result?.angles?.[i];
    const typed = figure?.angles[i];
    if (part && part.status !== "unknown") return { status: part.status, text: short(part.value), part };
    if (typed) return { status: "given", text: typed, part: null };
    return { status: "unknown", text: "?", part: null };
  }

  function editSide(i: number) {
    if (!figure || figure.kind === "circle") return;
    const n = shown.length;
    const mid: Pt = [(shown[i][0] + shown[(i + 1) % n][0]) / 2, (shown[i][1] + shown[(i + 1) % n][1]) / 2];
    const [x, y] = outward(mid, 22);
    editing = { type: "side", index: i, x, y, value: figure.sides[i] ?? "", which: "r" };
  }

  function editAngle(i: number) {
    if (!figure || figure.kind === "circle" || figure.kind === "rectangle" || figure.kind === "square") return;
    const [x, y] = outward(shown[i], -34);
    editing = { type: "angle", index: i, x, y, value: figure.angles[i] ?? "", which: "r" };
  }

  function editCircle() {
    if (!figure || figure.kind !== "circle") return;
    const [x, y] = toScreen([shown[0][0] + shownRadius / 2, shown[0][1]]);
    editing = { type: "circle", index: 0, x, y: y - 26, value: figure.circle?.value ?? "", which: figure.circle?.which ?? "r" };
  }

  function commit() {
    if (!editing || !figure) return;
    const v = editing.value.trim();
    if (editing.type === "side") figure.sides[editing.index] = v || null;
    else if (editing.type === "angle") figure.angles[editing.index] = v || null;
    else figure.circle = v ? { which: editing.which, value: v } : null;
    editing = null;
    changed();
  }

  function editKey(e: KeyboardEvent) {
    if (e.key === "Enter") commit();
    if (e.key === "Escape") editing = null;
    e.stopPropagation();
  }

  function focusInput(el: HTMLInputElement) {
    el.focus();
    el.select();
  }

  /** A step note: plain text with $…$ around inline maths. */
  function noteHtml(note: string) {
    const escape = (s: string) => s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
    return note
      .split("$")
      .map((part, k) => (k % 2 ? render(part) : escape(part)))
      .join("");
  }

  const shapeTitle = $derived.by(() => {
    if (!figure) return "";
    const names = shown.map((_, i) => corner(i)).join("");
    switch (figure.kind) {
      case "triangle":
        return `Triangle ${names}`;
      case "rectangle":
        return `Rectangle ${names}`;
      case "square":
        return `Square ${names}`;
      case "circle":
        return "Circle";
      default:
        return shown.length === 3 ? `Triangle ${names}` : `Polygon ${names}`;
    }
  });

  /** Interior angle arcs: the two directions at a corner, on screen. */
  function arcPath(i: number) {
    const n = shown.length;
    const p = toScreen(shown[i]);
    const a = toScreen(shown[(i + n - 1) % n]);
    const b = toScreen(shown[(i + 1) % n]);
    const ua = [a[0] - p[0], a[1] - p[1]];
    const ub = [b[0] - p[0], b[1] - p[1]];
    const la = Math.hypot(ua[0], ua[1]) || 1;
    const lb = Math.hypot(ub[0], ub[1]) || 1;
    const r = Math.min(18, la / 3, lb / 3);
    const pa = [p[0] + (ua[0] / la) * r, p[1] + (ua[1] / la) * r];
    const pb = [p[0] + (ub[0] / lb) * r, p[1] + (ub[1] / lb) * r];
    const info = angleInfo(i);
    if (info.part?.value != null && Math.abs(info.part.value - 90) < 1e-6) {
      // A right angle: a small square.
      const q = [pa[0] + pb[0] - p[0], pa[1] + pb[1] - p[1]];
      return `M ${pa[0]} ${pa[1]} L ${q[0]} ${q[1]} L ${pb[0]} ${pb[1]}`;
    }
    const cross = ua[0] * ub[1] - ua[1] * ub[0];
    const interior = info.part?.value ?? 180;
    const large = interior > 180 ? 1 : 0;
    const sweep = cross > 0 ? (large ? 0 : 1) : large ? 1 : 0;
    return `M ${pa[0]} ${pa[1]} A ${r} ${r} 0 ${large} ${sweep} ${pb[0]} ${pb[1]}`;
  }
</script>

<svelte:window onkeydown={keydown} />

<div class="geo">
  <div class="left">
    <div class="toolbar" role="toolbar" aria-label="Drawing tools">
      {#each TOOLS as t}
        <button class="tool" class:on={tool === t.id} title={`${t.label}: ${t.hint}`} onclick={() => ((tool = t.id), (draft = []))}>
          <span class="tool-icon">{t.icon}</span><span class="tool-label">{t.label}</span>
        </button>
      {/each}
      <span class="spacer"></span>
      {#if figure}<button class="ghost small" onclick={clearFigure} title="Remove the figure">Clear</button>{/if}
    </div>
    <div class="hint">{TOOLS.find((t) => t.id === tool)?.hint}{tool !== "select" ? " · hold Alt to place freely" : ""}</div>

    <div
      class="sheet"
      class:drawing={tool !== "select"}
      bind:this={box}
      role="application"
      aria-label="Drawing sheet"
      onpointerdown={pointerDown}
      onpointermove={pointerMove}
      onpointerup={pointerUp}
      ondblclick={() => tool === "polygon" && draft.length >= 3 && start("polygon", draft)}
    >
      <svg width={size.w} height={size.h}>
        {#each gridLines.xs as x}
          {@const sx = toScreen([x, 0])[0]}
          <line class="grid" x1={sx} y1="0" x2={sx} y2={size.h} />
        {/each}
        {#each gridLines.ys as y}
          {@const sy = toScreen([0, y])[1]}
          <line class="grid" x1="0" y1={sy} x2={size.w} y2={sy} />
        {/each}

        {#if figure && figure.kind === "circle" && shown.length}
          {@const c = toScreen(shown[0])}
          {@const rr = shownRadius * view.k}
          <circle class="shape" class:solved cx={c[0]} cy={c[1]} r={rr} />
          <line class="radius" x1={c[0]} y1={c[1]} x2={c[0] + rr} y2={c[1]} />
          <circle class="dot" cx={c[0]} cy={c[1]} r="3" />
          <circle class="handle" cx={c[0] + rr} cy={c[1]} r="5" />
          <g class="label clickable" role="button" tabindex="0" onpointerdown={(e) => e.stopPropagation()} onclick={editCircle} onkeydown={(e) => e.key === "Enter" && editCircle()}>
            <text x={c[0] + rr / 2} y={c[1] - 8} class={figure.circle ? "given" : solved ? "found" : "unknown"} text-anchor="middle">
              {figure.circle ? `${figure.circle.which} = ${figure.circle.value} ${figure.circle.which === "A" ? unitText + "²" : unitText}` : "r = ?"}
            </text>
          </g>
        {:else if figure && shown.length >= 2}
          <polygon class="shape" class:solved points={shown.map((p) => toScreen(p).join(",")).join(" ")} />
          {#each shown as _, i}
            <path class="arc" class:fixed={angleInfo(i).status === "fixed"} d={arcPath(i)} />
          {/each}
          {#each shown as p, i}
            {@const n = shown.length}
            {@const mid = [(p[0] + shown[(i + 1) % n][0]) / 2, (p[1] + shown[(i + 1) % n][1]) / 2] as Pt}
            {@const at = outward(mid, 16)}
            {@const info = sideInfo(i)}
            <g class="label clickable" role="button" tabindex="0" aria-label={`Side ${sideName(i, n)}`} onpointerdown={(e) => e.stopPropagation()} onclick={() => editSide(i)} onkeydown={(e) => e.key === "Enter" && editSide(i)}>
              <text x={at[0]} y={at[1]} class={info.status} text-anchor="middle" dominant-baseline="middle">
                {sideName(i, n)} = {info.text}{info.text !== "?" ? ` ${unitText}` : ""}
              </text>
            </g>
          {/each}
          {#each shown as p, i}
            {@const v = outward(p, 14)}
            <text class="corner" x={v[0]} y={v[1]} text-anchor="middle" dominant-baseline="middle">{corner(i)}</text>
            {@const inside = outward(p, -30)}
            {@const info = angleInfo(i)}
            <g class="label clickable" role="button" tabindex="0" aria-label={`Angle ${corner(i)}`} onpointerdown={(e) => e.stopPropagation()} onclick={() => editAngle(i)} onkeydown={(e) => e.key === "Enter" && editAngle(i)}>
              <text x={inside[0]} y={inside[1]} class={`angle ${info.status}`} text-anchor="middle" dominant-baseline="middle">
                {info.text}{info.text !== "?" ? "°" : ""}
              </text>
            </g>
            <circle class="handle" cx={toScreen(p)[0]} cy={toScreen(p)[1]} r="4.5" />
          {/each}
        {/if}

        {#if draft.length}
          {@const pts = hover ? [...draft, hover] : draft}
          <polyline class="draft" points={pts.map((p) => toScreen(p).join(",")).join(" ")} />
          {#each draft as p, i}
            <circle class="handle" cx={toScreen(p)[0]} cy={toScreen(p)[1]} r="4.5" />
            <text class="corner" x={toScreen(p)[0] + 10} y={toScreen(p)[1] - 10}>{corner(i)}</text>
          {/each}
        {/if}

        {#if stretch}
          {#if tool === "circle"}
            {@const c = toScreen(stretch.from)}
            <circle class="draft" cx={c[0]} cy={c[1]} r={Math.hypot(stretch.to[0] - stretch.from[0], stretch.to[1] - stretch.from[1]) * view.k} />
          {:else}
            <polygon class="draft" points={boxCorners(stretch.from, stretch.to, tool === "square").map((p) => toScreen(p).join(",")).join(" ")} />
          {/if}
        {/if}
      </svg>

      {#if !figure && !draft.length && !stretch}
        <div class="empty">Pick a tool and draw a shape. Then click a side or an angle to give its value.</div>
      {/if}

      {#if editing}
        <div class="editor" style:left={`${editing.x}px`} style:top={`${editing.y}px`} onpointerdown={(e) => e.stopPropagation()} role="dialog" tabindex="-1" aria-label="Give a value">
          {#if editing.type === "circle"}
            <select bind:value={editing.which} aria-label="Which measurement">
              <option value="r">radius r</option>
              <option value="d">diameter d</option>
              <option value="C">circumference C</option>
              <option value="A">area A</option>
            </select>
          {:else}
            <span class="editor-name">{editing.type === "side" ? sideName(editing.index, shown.length) : corner(editing.index)} =</span>
          {/if}
          <input use:focusInput bind:value={editing.value} onkeydown={editKey} placeholder={editing.type === "angle" ? "e.g. 90" : "e.g. 20"} aria-label="Value" />
          <span class="editor-unit">{editing.type === "angle" ? "°" : editing.type === "circle" && editing.which === "A" ? unitText + "²" : unitText}</span>
          <button class="small" onclick={commit}>OK</button>
        </div>
      {/if}
    </div>
  </div>

  <aside class="inspector" aria-label="Inspector">
    {#if !figure}
      <div class="inspector-empty">
        <h3>Inspector</h3>
        <p>Draw a triangle, rectangle, square, polygon or circle. Give the sides and angles you know (leave the rest blank) and ProCalc works out the others, draws the figure to scale, and offers its area, perimeter and more, with the formula and how it was found.</p>
      </div>
    {:else}
      <h3>{shapeTitle}</h3>
      {#if result}
        <div class="status" class:ok={result.status === "solved"} class:warn={result.status === "missing"} class:bad={result.status === "conflict" || result.status === "error"}>
          {#if result.status === "solved"}✓ Solved, drawn to scale{:else if result.status === "missing"}Needs more{:else}Doesn't fit{/if}
          {#if result.message}<div class="status-msg">{result.message}</div>{/if}
        </div>
      {/if}

      {#if figure.kind !== "circle"}
        <div class="section">Sides</div>
        {#each shown as _, i}
          {@const info = sideInfo(i)}
          <button class="row" onclick={() => editSide(i)}>
            <span class="row-name">{sideName(i, shown.length)}</span>
            <span class="row-value">
              {#if info.part?.latex}{@html render(info.part.latex + unitLatex(1))}{:else if info.status === "given"}{info.text} {unitText}{:else}?{/if}
            </span>
            <span class="chip {info.status}">{info.status === "unknown" ? "unknown" : info.status}</span>
          </button>
        {/each}
        <div class="section">Angles</div>
        {#each shown as _, i}
          {@const info = angleInfo(i)}
          <button class="row" onclick={() => editAngle(i)} disabled={info.status === "fixed"}>
            <span class="row-name">{corner(i)}</span>
            <span class="row-value">
              {#if info.part?.latex}{@html render(info.part.latex + "^{\\circ}")}{:else if info.status === "given"}{info.text}°{:else}?{/if}
            </span>
            <span class="chip {info.status}">{info.status === "fixed" ? "90° (fixed)" : info.status}</span>
          </button>
        {/each}
      {/if}

      {#if result?.quantities.length}
        <div class="section">What do you want?</div>
        <div class="wants">
          {#each result.quantities as q}
            <button class="want" class:on={openCards[q.id]} onclick={() => (openCards[q.id] = !openCards[q.id])}>{q.name}</button>
          {/each}
        </div>
        {#each result.quantities.filter((q) => openCards[q.id]) as q}
          <div class="card" title={q.dim === 2 ? areaTip(q.value) : ""}>
            <div class="card-head">{q.name}</div>
            <div class="card-value">{@html render(q.latex + unitLatex(q.dim))}</div>
            {#if q.text !== q.latex}<div class="card-decimal">≈ {withUnit(short(q.value), q.dim)}</div>{/if}
            {#if q.dim === 2 && areaTip(q.value)}<div class="card-tip">{areaTip(q.value)}</div>{/if}
            <div class="card-formula">Formula: {@html render(q.formula)}</div>
            <ol class="card-steps">
              {#each q.steps as s}
                <li>
                  {#if s.latex}<div class="step-math">{@html render(s.latex)}</div>{/if}
                  {#if s.note}<div class="step-note">{@html noteHtml(s.note)}</div>{/if}
                </li>
              {/each}
            </ol>
          </div>
        {/each}
      {/if}
    {/if}
  </aside>
</div>

<style>
  .geo {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 340px;
    gap: 12px;
    height: 100%;
    min-height: 0;
    padding: 4px 12px 12px;
  }

  .left {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }

  .toolbar {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-wrap: wrap;
  }

  .tool {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
    border: 1px solid var(--line);
    border-radius: 9px;
    background: var(--panel);
    color: inherit;
    font: inherit;
    font-size: 13px;
    cursor: pointer;
  }

  .tool.on {
    border-color: var(--accent-strong);
    background: color-mix(in srgb, var(--accent-strong) 14%, var(--panel));
  }

  .tool-icon {
    font-size: 15px;
  }

  .spacer {
    flex: 1;
  }

  .hint {
    margin: 6px 2px;
    color: var(--muted);
    font-size: 12.5px;
  }

  .sheet {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow: hidden;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--panel);
    touch-action: none;
  }

  .sheet.drawing {
    cursor: crosshair;
  }

  svg {
    position: absolute;
    inset: 0;
    display: block;
  }

  .grid {
    stroke: var(--line);
    stroke-width: 1;
    opacity: 0.6;
  }

  .shape {
    fill: color-mix(in srgb, var(--accent-strong) 8%, transparent);
    stroke: var(--muted);
    stroke-width: 2;
    stroke-dasharray: 6 4;
  }

  .shape.solved {
    stroke: var(--accent-strong);
    stroke-dasharray: none;
  }

  .draft {
    fill: none;
    stroke: var(--accent-strong);
    stroke-width: 1.5;
    stroke-dasharray: 4 4;
  }

  .radius {
    stroke: var(--muted);
    stroke-width: 1.5;
    stroke-dasharray: 3 3;
  }

  .dot {
    fill: var(--muted);
  }

  .handle {
    fill: var(--panel);
    stroke: var(--accent-strong);
    stroke-width: 2;
  }

  .arc {
    fill: none;
    stroke: var(--muted);
    stroke-width: 1.3;
  }

  .corner {
    fill: currentColor;
    font-size: 13px;
    font-weight: 700;
    pointer-events: none;
  }

  .label text {
    font-size: 12.5px;
    paint-order: stroke;
    stroke: var(--panel);
    stroke-width: 4px;
    stroke-linejoin: round;
  }

  .label.clickable {
    cursor: pointer;
  }

  .label.clickable:hover text {
    text-decoration: underline;
  }

  text.given {
    fill: var(--accent-strong);
    font-weight: 650;
  }

  text.found {
    fill: currentColor;
  }

  text.fixed {
    fill: var(--muted);
  }

  text.unknown {
    fill: var(--muted);
    font-style: italic;
  }

  text.angle {
    font-size: 11.5px;
  }

  .empty {
    position: absolute;
    inset: 0;
    display: grid;
    place-items: center;
    padding: 24px;
    color: var(--muted);
    text-align: center;
    pointer-events: none;
  }

  .editor {
    position: absolute;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    border: 1px solid var(--accent-strong);
    border-radius: 10px;
    background: var(--panel);
    box-shadow: 0 10px 28px rgb(0 0 0 / 0.3);
    transform: translate(-50%, -50%);
  }

  .editor input {
    width: 90px;
  }

  .editor-name,
  .editor-unit {
    color: var(--muted);
    font-size: 13px;
  }

  .inspector {
    overflow-y: auto;
    padding: 12px 14px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: var(--panel);
  }

  .inspector h3 {
    margin: 0 0 8px;
    font-size: 15px;
  }

  .inspector-empty p {
    color: var(--muted);
    font-size: 13px;
    line-height: 1.5;
  }

  .status {
    margin-bottom: 8px;
    padding: 8px 10px;
    border-radius: 9px;
    background: var(--panel-2);
    font-size: 13px;
    font-weight: 600;
  }

  .status.ok {
    color: var(--accent-strong);
  }

  .status.warn {
    color: #d9a400;
  }

  .status.bad {
    color: #e5484d;
  }

  .status-msg {
    margin-top: 2px;
    color: inherit;
    font-weight: 400;
  }

  .section {
    margin: 14px 0 4px;
    color: var(--muted);
    font-size: 11px;
    letter-spacing: 0.4px;
    text-transform: uppercase;
  }

  .row {
    display: grid;
    grid-template-columns: 48px minmax(0, 1fr) auto;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 6px 6px;
    border: 0;
    border-radius: 8px;
    background: transparent;
    color: inherit;
    font: inherit;
    text-align: left;
    cursor: pointer;
  }

  .row:hover:not(:disabled) {
    background: var(--panel-2);
  }

  .row:disabled {
    cursor: default;
  }

  .row-name {
    font-weight: 650;
  }

  .row-value {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chip {
    padding: 1px 7px;
    border-radius: 999px;
    border: 1px solid var(--line);
    color: var(--muted);
    font-size: 11px;
  }

  .chip.given {
    border-color: var(--accent-strong);
    color: var(--accent-strong);
  }

  .chip.found {
    background: var(--panel-2);
    color: inherit;
  }

  .wants {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .want {
    padding: 4px 10px;
    border: 1px solid var(--line);
    border-radius: 999px;
    background: transparent;
    color: inherit;
    font: inherit;
    font-size: 12.5px;
    cursor: pointer;
  }

  .want.on {
    border-color: var(--accent-strong);
    background: color-mix(in srgb, var(--accent-strong) 14%, transparent);
  }

  .card {
    margin-top: 10px;
    padding: 10px 12px;
    border: 1px solid var(--line);
    border-left: 3px solid var(--accent-strong);
    border-radius: 10px;
  }

  .card-head {
    color: var(--muted);
    font-size: 12px;
    font-weight: 600;
  }

  .card-value {
    margin: 4px 0;
    font-size: 18px;
  }

  .card-decimal,
  .card-tip {
    color: var(--muted);
    font-size: 12.5px;
  }

  .card-formula {
    margin-top: 6px;
    font-size: 13px;
  }

  .card-steps {
    margin: 6px 0 0;
    padding-left: 18px;
    font-size: 12.5px;
  }

  .card-steps li {
    margin: 4px 0;
  }

  .step-note {
    color: var(--muted);
  }

  @media (max-width: 860px) {
    .geo {
      grid-template-columns: 1fr;
      grid-template-rows: minmax(300px, 1fr) auto;
    }
  }
</style>
