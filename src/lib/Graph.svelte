<script lang="ts">
  import { onMount } from "svelte";
  import type { Curve, View } from "$lib/engine";
  import { prefersDark, slotColor } from "$lib/palette";

  export interface Series {
    /** Stable key (the Paper line id). */
    id: number;
    /** Colour slot, fixed for the line. */
    slot: number;
    /** The Paper line number, for labels. */
    number: number;
    curve: Curve;
  }

  type Rect = { xMin: number; xMax: number; yMin: number; yMax: number };

  let {
    series,
    data,
    view = $bindable(),
    render,
    onrange,
  }: {
    series: Series[];
    /** The rectangle the samples in `series` were taken over. */
    data: Rect;
    view: Rect;
    render: (latex: string) => string;
    /** Asks for fresh samples over a view at this pixel size. */
    onrange: (view: View) => void;
  } = $props();

  let canvas = $state<HTMLCanvasElement | undefined>();
  let box = $state<HTMLDivElement | undefined>();
  let width = $state(600);
  let height = $state(260);
  /** The theme in use: curve colours and the canvas follow it. */
  const dark = $derived(prefersDark());
  let hidden = $state<Set<number>>(new Set());
  let hover = $state<{ px: number; py: number } | null>(null);
  /** 3D view of surfaces: rotation (radians) and zoom. */
  let view3d = $state({ az: -0.7, el: 0.55, zoom: 1 });
  type Style3d = "shaded" | "heat" | "wire";
  const OPTS_KEY = "procalc.graph3d.v1";
  const DEFAULT_3D = { mesh: true, style: "shaded" as Style3d, box: true, spin: false, perspective: false };

  /** Saved 3D options and the 2D/3D choice. */
  function loadOptions() {
    try {
      const saved = JSON.parse(localStorage.getItem(OPTS_KEY) ?? "null");
      if (saved && typeof saved === "object") {
        const { prefer3d: p, ...rest } = saved;
        return { opts: { ...DEFAULT_3D, ...rest }, prefer3d: typeof p === "boolean" ? p : true };
      }
    } catch {
      /* first run, or storage unavailable */
    }
    return { opts: { ...DEFAULT_3D }, prefer3d: true };
  }
  const loaded = loadOptions();

  /** 2D or 3D, when the sheet has a surface (in 2D a surface is a heat map). */
  let prefer3d = $state(loaded.prefer3d);
  let hasSurface = $derived(series.some((s) => s.curve.kind === "surface" && !hidden.has(s.id)));
  let is3d = $derived(hasSurface && prefer3d);
  /** 3D drawing options, remembered between launches. */
  let opts3d = $state(loaded.opts);
  $effect(() => {
    const snapshot = { ...opts3d, prefer3d };
    try {
      localStorage.setItem(OPTS_KEY, JSON.stringify(snapshot));
    } catch {
      /* storage unavailable: options just won't persist */
    }
  });

  // Spin: turn slowly while the 3D view is showing.
  $effect(() => {
    if (!(is3d && opts3d.spin)) return;
    let frame = 0;
    let last = performance.now();
    const tick = (now: number) => {
      view3d.az += (now - last) * 0.0004;
      last = now;
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  });

  /** A height in [0, 1] as a shade of the curve's colour: dark low, light high. */
  function heatColor(base: [number, number, number], t: number, light = 1): string {
    const k = 0.35 + 0.95 * Math.max(0, Math.min(1, t));
    const ch = (c: number) => Math.round(Math.min(255, (k <= 1 ? c * k : c + (255 - c) * (k - 1)) * light));
    return `rgb(${ch(base[0])},${ch(base[1])},${ch(base[2])})`;
  }

  /** The middle 96% of a surface's values (so a spike doesn't flatten the colours). */
  function zRange(zs: (number | null)[]): [number, number] {
    const v = zs.filter((z): z is number => z !== null).sort((a, b) => a - b);
    if (!v.length) return [-1, 1];
    let lo = v[Math.floor(v.length * 0.02)];
    let hi = v[Math.ceil(v.length * 0.98) - 1];
    if (hi - lo < 1e-9) [lo, hi] = [lo - 1, hi + 1];
    return [lo, hi];
  }

  /** z of a surface at (x, y), from the nearest grid point. */
  function surfaceAt(c: Extract<Curve, { kind: "surface" }>, x: number, y: number): number | null {
    if (x < c.x[0] || x > c.x[1] || y < c.y[0] || y > c.y[1]) return null;
    const i = Math.round(((x - c.x[0]) / (c.x[1] - c.x[0])) * c.nx);
    const j = Math.round(((y - c.y[0]) / (c.y[1] - c.y[0])) * c.ny);
    return c.zs[j * (c.nx + 1) + i];
  }

  const PAD = { left: 44, right: 34, top: 10, bottom: 24 };

  onMount(() => {
    const ro = new ResizeObserver(([e]) => {
      width = Math.max(200, Math.round(e.contentRect.width));
      height = Math.max(140, Math.round(e.contentRect.height));
      requestRange();
    });
    if (box) ro.observe(box);
    return () => {
      ro.disconnect();
    };
  });

  const plotWidth = () => Math.max(50, width - PAD.left - PAD.right);
  const plotHeight = () => Math.max(50, height - PAD.top - PAD.bottom);
  const toPx = (x: number) => PAD.left + ((x - view.xMin) / (view.xMax - view.xMin)) * plotWidth();
  const toPy = (y: number) => PAD.top + ((view.yMax - y) / (view.yMax - view.yMin)) * plotHeight();
  const fromPx = (px: number) => view.xMin + ((px - PAD.left) / plotWidth()) * (view.xMax - view.xMin);

  /** x of sample k of n across the sampled rectangle. */
  const sampleX = (k: number, n: number) => data.xMin + ((data.xMax - data.xMin) * k) / Math.max(1, n - 1);

  const shown = () => series.filter((s) => !hidden.has(s.id));

  /** A 1–2–5 step giving roughly `count` ticks over `span`. */
  function niceStep(span: number, count: number) {
    const raw = span / Math.max(1, count);
    const mag = 10 ** Math.floor(Math.log10(raw));
    const f = raw / mag;
    return (f < 1.5 ? 1 : f < 3.5 ? 2 : f < 7.5 ? 5 : 10) * mag;
  }

  function fmt(v: number) {
    if (v === 0) return "0";
    const a = Math.abs(v);
    if (a >= 1e6 || a < 1e-4) return v.toExponential(3).replace(/\.?0+e/, "e");
    return Number(v.toPrecision(6)).toString();
  }

  /** A jump across most of the view between neighbouring samples is an asymptote, not a line. */
  function isJump(a: number, b: number) {
    return Math.abs(b - a) > (view.yMax - view.yMin) * 2 && Math.sign(a) !== Math.sign(b);
  }

  type Marker = { x: number; y: number; kind: "root" | "cross" | "touch"; text: string };

  /** Roots and crossings of y = f(x) curves, and tangent points. */
  let markers = $derived.by(() => {
    const out: Marker[] = [];
    const fns = shown().filter((s) => s.curve.kind === "function");
    const ysOf = (s: Series) => (s.curve.kind === "function" ? s.curve.ys : []);
    const scan = (n: number, f: (k: number) => number | null, kind: Marker["kind"], text: string, yAt: (x: number) => number) => {
      for (let k = 1; k < n && out.length < 120; k++) {
        const a = f(k - 1);
        const b = f(k);
        if (a === null || b === null || a === 0 || Math.sign(a) === Math.sign(b) || isJump(a, b)) continue;
        const x0 = sampleX(k - 1, n);
        const x = x0 + ((sampleX(k, n) - x0) * a) / (a - b);
        out.push({ x, y: yAt(x), kind, text });
      }
    };
    for (const s of fns) scan(ysOf(s).length, (k) => ysOf(s)[k], "root", `line ${s.number}`, () => 0);
    for (let i = 0; i < fns.length; i++) {
      for (let j = i + 1; j < fns.length; j++) {
        const [a, b] = [ysOf(fns[i]), ysOf(fns[j])];
        if (a.length !== b.length) continue;
        const diff = (k: number) => (a[k] === null || b[k] === null ? null : a[k]! - b[k]!);
        scan(a.length, diff, "cross", `lines ${fns[i].number} & ${fns[j].number}`, (x) => valueOf(a, x) ?? 0);
      }
    }
    for (const s of shown()) {
      if (s.curve.kind === "tangent") {
        out.push({ x: s.curve.x0, y: s.curve.y0, kind: "touch", text: `slope ${fmt(s.curve.slope)}` });
      }
    }
    return out;
  });

  // Draw whenever anything visible changes.
  $effect(() => {
    if (!canvas) return;
    void [series, data, view.xMin, view.xMax, view.yMin, view.yMax, width, height, dark, hidden.size, hover, markers];
    void [view3d.az, view3d.el, view3d.zoom, is3d, opts3d.mesh, opts3d.style, opts3d.box, opts3d.perspective];
    if (is3d) draw3d();
    else draw();
  });

  function rgb(hex: string): [number, number, number] {
    const n = parseInt(hex.slice(1), 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
  }

  /** Surfaces as shaded quads, far ones first, inside a wire box. */
  function draw3d() {
    const ctx = canvas!.getContext("2d")!;
    const dpr = window.devicePixelRatio || 1;
    canvas!.width = Math.round(width * dpr);
    canvas!.height = Math.round(height * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);
    const muted = cssVar("--muted");
    const gridColor = cssVar("--line");
    const surfs = shown().filter((s) => s.curve.kind === "surface") as (Series & { curve: Extract<Curve, { kind: "surface" }> })[];
    if (!surfs.length) return;

    // A common box: x, y from the surfaces' domains, z from their values (ignoring spikes).
    let [x0, x1, y0, y1] = [Infinity, -Infinity, Infinity, -Infinity];
    const zsAll: number[] = [];
    for (const s of surfs) {
      x0 = Math.min(x0, s.curve.x[0]);
      x1 = Math.max(x1, s.curve.x[1]);
      y0 = Math.min(y0, s.curve.y[0]);
      y1 = Math.max(y1, s.curve.y[1]);
      for (const z of s.curve.zs) if (z !== null) zsAll.push(z);
    }
    zsAll.sort((a, b) => a - b);
    let z0 = zsAll[Math.floor(zsAll.length * 0.02)] ?? -1;
    let z1 = zsAll[Math.ceil(zsAll.length * 0.98) - 1] ?? 1;
    if (z1 - z0 < 1e-9) [z0, z1] = [z0 - 1, z1 + 1];
    const nx = (x: number) => ((x - x0) / (x1 - x0)) * 2 - 1;
    const ny = (y: number) => ((y - y0) / (y1 - y0)) * 2 - 1;
    const nz = (z: number) => Math.max(-1.4, Math.min(1.4, ((z - z0) / (z1 - z0)) * 2 - 1));

    const { az, el, zoom } = view3d;
    const [ca, sa, ce, se] = [Math.cos(az), Math.sin(az), Math.cos(el), Math.sin(el)];
    const scale = (Math.min(width, height) / 3.6) * zoom;
    const cx = width / 2;
    const cy = height / 2 + 6;
    // Turn by the azimuth, tilt by the elevation; depth grows away from the viewer.
    const project = (X: number, Y: number, Z: number) => {
      const xr = X * ca - Y * sa;
      const yr = X * sa + Y * ca;
      const depth = yr * ce - Z * se;
      // Perspective: nearer points spread out, farther ones draw in.
      const f = opts3d.perspective ? 1 / (1 + depth * 0.32) : 1;
      return { px: cx + xr * scale * f, py: cy - (yr * se + Z * ce) * scale * f, depth };
    };

    // The box behind everything.
    const corners = [-1, 1].flatMap((X) => [-1, 1].flatMap((Y) => [-1, 1].map((Z) => ({ X, Y, Z }))));
    ctx.strokeStyle = gridColor;
    ctx.lineWidth = 1;
    for (const a of opts3d.box ? corners : []) {
      for (const b of corners) {
        const diff = Math.abs(a.X - b.X) + Math.abs(a.Y - b.Y) + Math.abs(a.Z - b.Z);
        if (diff !== 2 || a.X + a.Y + a.Z > b.X + b.Y + b.Z) continue;
        const p = project(a.X, a.Y, a.Z);
        const q = project(b.X, b.Y, b.Z);
        ctx.beginPath();
        ctx.moveTo(p.px, p.py);
        ctx.lineTo(q.px, q.py);
        ctx.stroke();
      }
    }

    // Every quad of every surface, sorted far to near.
    const light = [0.35, -0.45, 0.82];
    type Quad = { pts: { px: number; py: number }[]; depth: number; color: [number, number, number]; shade: number; t: number };
    const quads: Quad[] = [];
    for (const s of surfs) {
      const c = s.curve;
      const at = (i: number, j: number) => c.zs[j * (c.nx + 1) + i];
      const X = (i: number) => nx(c.x[0] + ((c.x[1] - c.x[0]) * i) / c.nx);
      const Y = (j: number) => ny(c.y[0] + ((c.y[1] - c.y[0]) * j) / c.ny);
      const color = rgb(slotColor(s.slot, dark));
      for (let j = 0; j < c.ny; j++) {
        for (let i = 0; i < c.nx; i++) {
          const zz = [at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)];
          if (zz.some((z) => z === null)) continue;
          const P = [
            [X(i), Y(j), nz(zz[0]!)],
            [X(i + 1), Y(j), nz(zz[1]!)],
            [X(i + 1), Y(j + 1), nz(zz[2]!)],
            [X(i), Y(j + 1), nz(zz[3]!)],
          ];
          const u = [P[1][0] - P[0][0], P[1][1] - P[0][1], P[1][2] - P[0][2]];
          const v = [P[3][0] - P[0][0], P[3][1] - P[0][1], P[3][2] - P[0][2]];
          const nrm = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
          const len = Math.hypot(nrm[0], nrm[1], nrm[2]) || 1;
          const lambert = Math.abs((nrm[0] * light[0] + nrm[1] * light[1] + nrm[2] * light[2]) / len);
          const pts = P.map(([a, b, cc]) => project(a, b, cc));
          const t = ((zz[0]! + zz[1]! + zz[2]! + zz[3]!) / 4 - z0) / (z1 - z0);
          quads.push({ pts, depth: pts.reduce((sum, p) => sum + p.depth, 0) / 4, color, shade: 0.45 + 0.55 * lambert, t });
        }
      }
    }
    quads.sort((a, b) => b.depth - a.depth);
    const style = opts3d.style;
    // Mesh lines, when on, in a thin dark line; off, each face is outlined in its own colour,
    // which just hides the seams between faces.
    ctx.lineWidth = style === "wire" ? 1 : opts3d.mesh ? 0.6 : 0.8;
    for (const q of quads) {
      const fill =
        style === "heat"
          ? heatColor(q.color, q.t, 0.75 + 0.25 * q.shade)
          : `rgb(${q.color.map((c) => Math.round(c * q.shade)).join(",")})`;
      ctx.beginPath();
      ctx.moveTo(q.pts[0].px, q.pts[0].py);
      for (const p of q.pts.slice(1)) ctx.lineTo(p.px, p.py);
      ctx.closePath();
      if (style === "wire") {
        ctx.strokeStyle = heatColor(q.color, q.t);
        ctx.stroke();
        continue;
      }
      ctx.fillStyle = fill;
      ctx.strokeStyle = opts3d.mesh ? "rgba(0,0,0,0.18)" : fill;
      ctx.fill();
      ctx.stroke();
    }

    // Axis letters and ranges at the box edges.
    const axes = surfs[0].curve.axes;
    ctx.fillStyle = muted;
    ctx.font = "11px system-ui, -apple-system, 'Segoe UI', sans-serif";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    const label = (X: number, Y: number, Z: number, text: string) => {
      const p = project(X, Y, Z);
      ctx.fillText(text, p.px, p.py);
    };
    label(0, -1.25, -1, `${axes[0]}: ${fmt(x0)} … ${fmt(x1)}`);
    label(1.3, 0, -1, `${axes[1]}: ${fmt(y0)} … ${fmt(y1)}`);
    label(-1.15, -1.15, 1.15, `${axes[2]}: ${fmt(z0)} … ${fmt(z1)}`);
  }

  function cssVar(name: string) {
    return getComputedStyle(canvas!).getPropertyValue(name).trim();
  }

  /** The curve segments of F(x, y) = 0 on its grid (marching squares), in data units. */
  function contour(c: Extract<Curve, { kind: "implicit" }>): [number, number, number, number][] {
    const { nx, ny, values } = c;
    const at = (i: number, j: number) => values[j * (nx + 1) + i];
    const X = (i: number) => data.xMin + ((data.xMax - data.xMin) * i) / nx;
    const Y = (j: number) => data.yMin + ((data.yMax - data.yMin) * j) / ny;
    const segs: [number, number, number, number][] = [];
    for (let j = 0; j < ny; j++) {
      for (let i = 0; i < nx; i++) {
        const v = [at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)];
        if (v.some((x) => x === null)) continue;
        const [a, b, cc, d] = v as number[];
        // Where each edge crosses zero (bottom, right, top, left).
        const cross: [number, number][] = [];
        const edge = (p: number, q: number, x0: number, y0: number, x1: number, y1: number) => {
          if ((p < 0) !== (q < 0)) {
            const t = p / (p - q);
            cross.push([x0 + (x1 - x0) * t, y0 + (y1 - y0) * t]);
          }
        };
        edge(a, b, X(i), Y(j), X(i + 1), Y(j));
        edge(b, cc, X(i + 1), Y(j), X(i + 1), Y(j + 1));
        edge(cc, d, X(i + 1), Y(j + 1), X(i), Y(j + 1));
        edge(d, a, X(i), Y(j + 1), X(i), Y(j));
        // A cell spanning a pole (1/F flips sign through a huge value) isn't a curve.
        const big = Math.max(Math.abs(a), Math.abs(b), Math.abs(cc), Math.abs(d));
        const small = Math.min(Math.abs(a), Math.abs(b), Math.abs(cc), Math.abs(d));
        if (small > 0 && big / small > 1e6 && big > 1e6) continue;
        if (cross.length === 2) segs.push([...cross[0], ...cross[1]]);
        else if (cross.length === 4) {
          segs.push([...cross[0], ...cross[1]]);
          segs.push([...cross[2], ...cross[3]]);
        }
      }
    }
    return segs;
  }

  function draw() {
    const ctx = canvas!.getContext("2d")!;
    const dpr = window.devicePixelRatio || 1;
    canvas!.width = Math.round(width * dpr);
    canvas!.height = Math.round(height * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, width, height);
    const ink = cssVar("--ink");
    const muted = cssVar("--muted");
    const gridColor = cssVar("--line");
    const surface = cssVar("--panel");

    // Grid and tick labels.
    const xs = niceStep(view.xMax - view.xMin, plotWidth() / 90);
    const ysStep = niceStep(view.yMax - view.yMin, plotHeight() / 55);
    ctx.font = "11px system-ui, -apple-system, 'Segoe UI', sans-serif";
    ctx.lineWidth = 1;
    ctx.strokeStyle = gridColor;
    ctx.fillStyle = muted;
    ctx.textAlign = "center";
    ctx.textBaseline = "top";
    for (let x = Math.ceil(view.xMin / xs) * xs; x <= view.xMax; x += xs) {
      const px = Math.round(toPx(x)) + 0.5;
      ctx.beginPath();
      ctx.moveTo(px, PAD.top);
      ctx.lineTo(px, PAD.top + plotHeight());
      ctx.stroke();
      ctx.fillText(fmt(Math.abs(x) < xs / 1e6 ? 0 : x), px, PAD.top + plotHeight() + 6);
    }
    ctx.textAlign = "right";
    ctx.textBaseline = "middle";
    for (let y = Math.ceil(view.yMin / ysStep) * ysStep; y <= view.yMax; y += ysStep) {
      const py = Math.round(toPy(y)) + 0.5;
      ctx.beginPath();
      ctx.moveTo(PAD.left, py);
      ctx.lineTo(PAD.left + plotWidth(), py);
      ctx.stroke();
      ctx.fillText(fmt(Math.abs(y) < ysStep / 1e6 ? 0 : y), PAD.left - 6, py);
    }

    // Axes through 0, a little stronger than the grid.
    ctx.strokeStyle = muted;
    if (view.xMin < 0 && view.xMax > 0) {
      const px = Math.round(toPx(0)) + 0.5;
      ctx.beginPath();
      ctx.moveTo(px, PAD.top);
      ctx.lineTo(px, PAD.top + plotHeight());
      ctx.stroke();
    }
    if (view.yMin < 0 && view.yMax > 0) {
      const py = Math.round(toPy(0)) + 0.5;
      ctx.beginPath();
      ctx.moveTo(PAD.left, py);
      ctx.lineTo(PAD.left + plotWidth(), py);
      ctx.stroke();
    }

    // Everything plotted, clipped to the plot area.
    ctx.save();
    ctx.beginPath();
    ctx.rect(PAD.left, PAD.top, plotWidth(), plotHeight());
    ctx.clip();
    const inPlot = (px: number, py: number) =>
      px >= PAD.left && px <= PAD.left + plotWidth() && py >= PAD.top && py <= PAD.top + plotHeight();
    const pin = (py: number) => Math.max(-1e4, Math.min(1e4, py));
    // Surfaces in 2D: a heat map, under everything else.
    for (const s of shown()) {
      const c = s.curve;
      if (c.kind !== "surface") continue;
      const base = rgb(slotColor(s.slot, dark));
      const [lo, hi] = zRange(c.zs);
      const cw = (c.x[1] - c.x[0]) / c.nx;
      const chh = (c.y[1] - c.y[0]) / c.ny;
      for (let j = 0; j <= c.ny; j++) {
        for (let i = 0; i <= c.nx; i++) {
          const z = c.zs[j * (c.nx + 1) + i];
          if (z === null) continue;
          ctx.fillStyle = heatColor(base, (z - lo) / (hi - lo));
          const px0 = toPx(c.x[0] + cw * (i - 0.5));
          const py0 = toPy(c.y[0] + chh * (j + 0.5));
          ctx.fillRect(px0, py0, toPx(c.x[0] + cw * (i + 0.5)) - px0 + 0.5, toPy(c.y[0] + chh * (j - 0.5)) - py0 + 0.5);
        }
      }
    }
    const ends: { s: Series; px: number; py: number }[] = [];
    for (const s of shown()) {
      const color = slotColor(s.slot, dark);
      const c = s.curve;
      let last: { px: number; py: number } | null = null;
      ctx.lineWidth = 2;
      ctx.lineJoin = "round";
      ctx.lineCap = "round";
      ctx.strokeStyle = color;
      ctx.setLineDash(c.kind === "tangent" ? [6, 5] : []);
      if (c.kind === "function") {
        // The integral's area first, under the curve.
        if (c.shade) {
          const [a, b] = c.shade[0] <= c.shade[1] ? c.shade : [c.shade[1], c.shade[0]];
          ctx.fillStyle = color + "38";
          ctx.beginPath();
          let open = false;
          const zero = toPy(0);
          c.ys.forEach((y, k) => {
            const x = sampleX(k, c.ys.length);
            if (x < a || x > b || y === null) {
              if (open) {
                ctx.lineTo(toPx(sampleX(k - 1, c.ys.length)), zero);
                ctx.closePath();
                open = false;
              }
              return;
            }
            if (!open) {
              ctx.moveTo(toPx(x), zero);
              open = true;
            }
            ctx.lineTo(toPx(x), pin(toPy(y)));
          });
          if (open) {
            ctx.lineTo(toPx(Math.min(b, sampleX(c.ys.length - 1, c.ys.length))), zero);
            ctx.closePath();
          }
          ctx.fill();
        }
        ctx.beginPath();
        let pen = false;
        let prev: number | null = null;
        c.ys.forEach((y, k) => {
          if (y === null || (prev !== null && isJump(prev, y))) {
            pen = false;
            prev = y;
            if (y === null) return;
          }
          const px = toPx(sampleX(k, c.ys.length));
          const py = pin(toPy(y));
          if (pen) ctx.lineTo(px, py);
          else ctx.moveTo(px, py);
          pen = true;
          prev = y;
          if (inPlot(px, py)) last = { px, py };
        });
        ctx.stroke();
      } else if (c.kind === "path") {
        ctx.beginPath();
        let pen = false;
        for (const p of c.points) {
          if (!p) {
            pen = false;
            continue;
          }
          const px = toPx(p[0]);
          const py = pin(toPy(p[1]));
          if (pen) ctx.lineTo(px, py);
          else ctx.moveTo(px, py);
          pen = true;
          if (inPlot(px, py)) last = { px, py };
        }
        ctx.stroke();
      } else if (c.kind === "implicit") {
        if (c.region !== null) {
          // An inequality: shade the cells where it holds, edge dashed when strict.
          ctx.fillStyle = color + "33";
          const cw = (data.xMax - data.xMin) / c.nx;
          const chh = (data.yMax - data.yMin) / c.ny;
          const inside = (v: number | null) => v !== null && (c.region ? v > 0 : v >= 0);
          for (let j = 0; j < c.ny; j++) {
            for (let i = 0; i < c.nx; i++) {
              const corners = [c.values[j * (c.nx + 1) + i], c.values[j * (c.nx + 1) + i + 1], c.values[(j + 1) * (c.nx + 1) + i], c.values[(j + 1) * (c.nx + 1) + i + 1]];
              if (corners.filter(inside).length < 2) continue;
              const px0 = toPx(data.xMin + cw * i);
              const py0 = toPy(data.yMin + chh * (j + 1));
              // Overlap by half a pixel so neighbouring cells leave no seams.
              ctx.fillRect(px0 - 0.25, py0 - 0.25, toPx(data.xMin + cw * (i + 1)) - px0 + 0.5, toPy(data.yMin + chh * j) - py0 + 0.5);
            }
          }
          if (c.region) ctx.setLineDash([6, 4]);
        }
        ctx.beginPath();
        for (const [x0, y0, x1, y1] of contour(c)) {
          ctx.moveTo(toPx(x0), toPy(y0));
          ctx.lineTo(toPx(x1), toPy(y1));
          const px = toPx(x1);
          const py = toPy(y1);
          if (inPlot(px, py) && (!last || px > last.px)) last = { px, py };
        }
        ctx.stroke();
      } else if (c.kind === "tangent") {
        // Tangent: a line across the whole view.
        const yAt = (x: number) => c.y0 + c.slope * (x - c.x0);
        ctx.beginPath();
        ctx.moveTo(toPx(view.xMin), pin(toPy(yAt(view.xMin))));
        ctx.lineTo(toPx(view.xMax), pin(toPy(yAt(view.xMax))));
        ctx.stroke();
        const px = toPx(view.xMax) - 4;
        const py = toPy(yAt(fromPx(px)));
        if (inPlot(px, py)) last = { px, py };
      }
      ctx.setLineDash([]);
      if (last) ends.push({ s, ...(last as { px: number; py: number }) });
    }

    // Roots (filled, on the axis), crossings (rings) and tangent points, each ringed in surface.
    for (const m of markers) {
      const px = toPx(m.x);
      const py = toPy(m.y);
      ctx.beginPath();
      ctx.arc(px, py, 4.5, 0, Math.PI * 2);
      ctx.fillStyle = m.kind === "cross" ? surface : ink;
      ctx.strokeStyle = m.kind === "cross" ? ink : surface;
      ctx.lineWidth = 2;
      ctx.fill();
      ctx.stroke();
    }

    // Hover: crosshair and a dot on each y = f(x) curve.
    if (hover) {
      const x = fromPx(hover.px);
      ctx.strokeStyle = muted;
      ctx.lineWidth = 1;
      ctx.setLineDash([3, 3]);
      ctx.beginPath();
      ctx.moveTo(Math.round(hover.px) + 0.5, PAD.top);
      ctx.lineTo(Math.round(hover.px) + 0.5, PAD.top + plotHeight());
      ctx.stroke();
      ctx.setLineDash([]);
      for (const s of shown()) {
        const y = seriesValue(s, x);
        if (y === null) continue;
        ctx.beginPath();
        ctx.arc(hover.px, toPy(y), 4, 0, Math.PI * 2);
        ctx.fillStyle = slotColor(s.slot, dark);
        ctx.strokeStyle = surface;
        ctx.lineWidth = 2;
        ctx.fill();
        ctx.stroke();
      }
    }
    ctx.restore();

    // Direct labels at each curve's last visible point: the line number, in ink.
    ctx.font = "600 11px system-ui, -apple-system, 'Segoe UI', sans-serif";
    ctx.textAlign = "left";
    ctx.textBaseline = "middle";
    const used: number[] = [];
    for (const { s, px, py } of ends) {
      let y = Math.min(Math.max(py, PAD.top + 8), PAD.top + plotHeight() - 8);
      while (used.some((u) => Math.abs(u - y) < 14)) y += 14;
      used.push(y);
      const x = Math.min(px + 6, width - PAD.right + 4);
      ctx.fillStyle = slotColor(s.slot, dark);
      ctx.fillRect(x, y - 1, 6, 2);
      ctx.fillStyle = ink;
      ctx.fillText(String(s.number), x + 9, y);
    }
  }

  /** Nearest-sample value of evenly spaced samples at x. */
  function valueOf(ys: (number | null)[], x: number): number | null {
    const n = ys.length;
    if (n < 2 || x < data.xMin || x > data.xMax) return null;
    const k = Math.round(((x - data.xMin) / (data.xMax - data.xMin)) * (n - 1));
    return ys[Math.max(0, Math.min(n - 1, k))];
  }

  /** The value at x of a curve that has one (functions and tangents). */
  function seriesValue(s: Series, x: number): number | null {
    const c = s.curve;
    if (c.kind === "function") return valueOf(c.ys, x);
    if (c.kind === "tangent") return c.y0 + c.slope * (x - c.x0);
    return null;
  }

  // ---------- Interaction ----------

  const pointers = new Map<number, { x: number; y: number }>();
  let pinchStart: { dist: number; view: Rect } | null = null;
  let dragStart: { x: number; y: number; view: Rect } | null = null;

  function local(e: PointerEvent | WheelEvent | MouseEvent) {
    const r = canvas!.getBoundingClientRect();
    return { x: e.clientX - r.left, y: e.clientY - r.top };
  }

  function requestRange() {
    onrange({ ...view, width: plotWidth(), height: plotHeight() });
  }

  let rangeTimer: ReturnType<typeof setTimeout> | undefined;
  function requestRangeSoon() {
    clearTimeout(rangeTimer);
    rangeTimer = setTimeout(requestRange, 90);
  }

  function onPointerDown(e: PointerEvent) {
    canvas!.setPointerCapture(e.pointerId);
    pointers.set(e.pointerId, local(e));
    if (pointers.size === 1) {
      dragStart = { ...local(e), view: { ...view } };
    } else if (pointers.size === 2) {
      const [a, b] = [...pointers.values()];
      pinchStart = { dist: Math.hypot(a.x - b.x, a.y - b.y), view: { ...view } };
      dragStart = null;
    }
  }

  function onPointerMove(e: PointerEvent) {
    const p = local(e);
    if (pointers.has(e.pointerId)) pointers.set(e.pointerId, p);
    if (is3d) {
      // Drag turns the surface.
      if (dragStart) {
        view3d.az += (p.x - dragStart.x) * 0.01;
        view3d.el = Math.max(-1.45, Math.min(1.45, view3d.el + (p.y - dragStart.y) * 0.01));
        dragStart = { ...p, view: dragStart.view };
      }
      return;
    }
    if (pinchStart && pointers.size === 2) {
      const [a, b] = [...pointers.values()];
      const factor = pinchStart.dist / Math.max(10, Math.hypot(a.x - b.x, a.y - b.y));
      zoomAround((a.x + b.x) / 2, (a.y + b.y) / 2, factor, pinchStart.view);
      return;
    }
    if (dragStart) {
      const v = dragStart.view;
      const dx = ((p.x - dragStart.x) / plotWidth()) * (v.xMax - v.xMin);
      const dy = ((p.y - dragStart.y) / plotHeight()) * (v.yMax - v.yMin);
      view = { xMin: v.xMin - dx, xMax: v.xMax - dx, yMin: v.yMin + dy, yMax: v.yMax + dy };
      hover = null;
      return;
    }
    hover = p.x >= PAD.left && p.x <= PAD.left + plotWidth() ? { px: p.x, py: p.y } : null;
  }

  function onPointerUp(e: PointerEvent) {
    pointers.delete(e.pointerId);
    if (pointers.size < 2) pinchStart = null;
    if (pointers.size === 0) {
      if (dragStart && !is3d) requestRange();
      dragStart = null;
    }
  }

  /** Zooms by `factor` (>1 out, <1 in) keeping the point under (px, py) still. */
  function zoomAround(px: number, py: number, factor: number, from: Rect = view, axes: "both" | "x" | "y" = "both") {
    const cx = from.xMin + ((px - PAD.left) / plotWidth()) * (from.xMax - from.xMin);
    const cy = from.yMax - ((py - PAD.top) / plotHeight()) * (from.yMax - from.yMin);
    const fx = axes === "y" ? 1 : factor;
    const fy = axes === "x" ? 1 : factor;
    const next = {
      xMin: cx - (cx - from.xMin) * fx,
      xMax: cx + (from.xMax - cx) * fx,
      yMin: cy - (cy - from.yMin) * fy,
      yMax: cy + (from.yMax - cy) * fy,
    };
    if (next.xMax - next.xMin < 1e-9 || next.xMax - next.xMin > 1e9) return;
    view = next;
    requestRangeSoon();
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
    if (is3d) {
      view3d.zoom = Math.max(0.3, Math.min(5, view3d.zoom * Math.pow(1.0015, -e.deltaY)));
      return;
    }
    const p = local(e);
    const axes = e.shiftKey ? "x" : e.altKey ? "y" : "both";
    zoomAround(p.x, p.y, Math.pow(1.0015, e.deltaY), view, axes);
  }

  /** Fits y to what the visible curves do (ignoring spikes near asymptotes). */
  export function fit() {
    if (is3d) {
      view3d = { az: -0.7, el: 0.55, zoom: 1 };
      return;
    }
    const values: number[] = [];
    for (const s of shown()) {
      const c = s.curve;
      if (c.kind === "function") {
        c.ys.forEach((y, k) => {
          const x = sampleX(k, c.ys.length);
          if (y !== null && x >= view.xMin && x <= view.xMax) values.push(y);
        });
      } else if (c.kind === "path") {
        for (const p of c.points) if (p) values.push(p[1]);
      } else if (c.kind === "tangent") {
        values.push(c.y0);
      }
    }
    if (!values.length) return;
    values.sort((a, b) => a - b);
    const lo = values[Math.floor(values.length * 0.03)];
    const hi = values[Math.ceil(values.length * 0.97) - 1];
    const span = hi - lo || Math.max(1, Math.abs(hi));
    view = { ...view, yMin: lo - span * 0.12, yMax: hi + span * 0.12 };
    requestRange();
  }

  /** −10…10 across, with the same scale up and down (circles look round). */
  export function reset() {
    const aspect = plotHeight() / plotWidth();
    view = { xMin: -10, xMax: 10, yMin: -10 * aspect, yMax: 10 * aspect };
    requestRange();
  }

  function toggle(id: number) {
    const next = new Set(hidden);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    hidden = next;
  }

  /** Tooltip rows at the hovered x, plus a marker if one is under the pointer. */
  let tip = $derived.by(() => {
    if (!hover) return null;
    const x = fromPx(hover.px);
    const near = markers.find((m) => Math.hypot(toPx(m.x) - hover!.px, toPy(m.y) - hover!.py) < 9);
    const rows = shown()
      .map((s) => ({ s, y: seriesValue(s, x) }))
      .filter((r) => r.y !== null);
    // Heat maps: z at the pointer.
    const y = view.yMax - ((hover.py - PAD.top) / plotHeight()) * (view.yMax - view.yMin);
    const heights = shown()
      .map((s) => ({ s, z: s.curve.kind === "surface" ? surfaceAt(s.curve, x, y) : null }))
      .filter((r) => r.z !== null);
    return { x, y, rows, heights, near };
  });

  const markerTitle = { root: "Root", cross: "Crossing", touch: "Tangent point" };
</script>

<div class="graph">
  <div class="legend">
    {#each series as s (s.id)}
      <button class="chip" class:off={hidden.has(s.id)} onclick={() => toggle(s.id)} title={hidden.has(s.id) ? "Show" : "Hide"}>
        <span class="swatch" class:dashed={s.curve.kind === "tangent"} style:--c={slotColor(s.slot, dark)}></span>
        <span class="num">{s.number}</span>
        <span class="tex">{@html render(s.curve.label)}</span>
      </button>
    {/each}
    {#if !series.length}
      <span class="hint">
        Plot <code>x^2</code>, <code>y = 2x+1</code>, <code>x^2+y^2=1</code>, <code>r = 1+\cos\theta</code>,
        <code>x=\cos t, y=\sin t</code>, <code>∫_0^2 x^2 dx</code> or <code>f'(1)</code>.
      </span>
    {/if}
    <span class="spacer"></span>
    {#if is3d}
      <div class="tools" role="group" aria-label="3D options">
        <button class:on={opts3d.mesh} onclick={() => (opts3d.mesh = !opts3d.mesh)} title="Mesh lines on the surface">Mesh</button>
        <div class="seg" role="group" aria-label="Surface style">
          <button class:on={opts3d.style === "shaded"} onclick={() => (opts3d.style = "shaded")} title="Lit by a light">Shaded</button>
          <button class:on={opts3d.style === "heat"} onclick={() => (opts3d.style = "heat")} title="Coloured by height: dark low, light high">Heat</button>
          <button class:on={opts3d.style === "wire"} onclick={() => (opts3d.style = "wire")} title="Wireframe only">Wire</button>
        </div>
        <button class:on={opts3d.box} onclick={() => (opts3d.box = !opts3d.box)} title="The bounding box">Box</button>
        <button class:on={opts3d.spin} onclick={() => (opts3d.spin = !opts3d.spin)} title="Turn slowly">Spin</button>
        <button class:on={opts3d.perspective} onclick={() => (opts3d.perspective = !opts3d.perspective)} title="Perspective instead of a flat projection">Persp</button>
      </div>
    {/if}
    {#if hasSurface}
      <div class="seg" role="group" aria-label="View">
        <button class:on={!is3d} onclick={() => (prefer3d = false)} title="Flat: surfaces as a heat map">2D</button>
        <button class:on={is3d} onclick={() => (prefer3d = true)}>3D</button>
      </div>
    {/if}
    <button class="ghost" onclick={fit} title={is3d ? "Back to the starting angle" : "Fit the height to the curves (or double-click the graph)"}>Fit</button>
    <button class="ghost" onclick={reset} title="Back to −10…10, same scale both ways">Reset</button>
  </div>
  <div class="plot" bind:this={box}>
    <canvas
      bind:this={canvas}
      style:width="{width}px"
      style:height="{height}px"
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onpointercancel={onPointerUp}
      onpointerleave={() => (hover = null)}
      onwheel={onWheel}
      ondblclick={fit}
      aria-label="Graph of the Paper's functions: drag to pan, scroll to zoom, double-click to fit"
    ></canvas>
    {#if tip && !is3d && (tip.rows.length || tip.near || tip.heights.length)}
      <div class="tip" style:left="{Math.min(hover!.px + 14, width - 190)}px" style:top="{Math.max(4, hover!.py - 12)}px">
        {#if tip.near}
          <div class="tip-head">{markerTitle[tip.near.kind]}: {tip.near.text}</div>
          <div class="tip-row">x ≈ {fmt(tip.near.x)}{tip.near.kind !== "root" ? `, y ≈ ${fmt(tip.near.y)}` : ""}</div>
        {:else}
          <div class="tip-head">x = {fmt(tip.x)}{tip.heights.length ? `, y = ${fmt(tip.y)}` : ""}</div>
          {#each tip.rows as r (r.s.id)}
            <div class="tip-row">
              <span class="swatch" style:--c={slotColor(r.s.slot, dark)}></span>
              <span class="num">{r.s.number}</span>
              <span>{fmt(r.y!)}</span>
            </div>
          {/each}
          {#each tip.heights as h (h.s.id)}
            <div class="tip-row">
              <span class="swatch" style:--c={slotColor(h.s.slot, dark)}></span>
              <span class="num">{h.s.number}</span>
              <span>z = {fmt(h.z!)}</span>
            </div>
          {/each}
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .graph {
    display: flex;
    flex-direction: column;
    min-height: 0;
    height: 100%;
  }

  .legend {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding: 0 0 6px;
  }

  .chip {
    display: flex;
    align-items: center;
    gap: 6px;
    background: var(--panel-2);
    border: 1px solid var(--line);
    border-radius: 999px;
    padding: 2px 10px 2px 8px;
    font-size: 13px;
    color: var(--ink);
    cursor: pointer;
  }

  .chip.off {
    opacity: 0.45;
  }

  .swatch {
    width: 10px;
    height: 10px;
    border-radius: 3px;
    flex: none;
    background: var(--c);
  }

  .swatch.dashed {
    background: repeating-linear-gradient(90deg, var(--c) 0 3px, transparent 3px 5px);
    height: 3px;
    width: 12px;
  }

  .num {
    font-size: 11px;
    font-weight: 600;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }

  .hint {
    font-size: 13px;
    color: var(--muted);
    line-height: 1.8;
  }

  .hint code {
    font-family: var(--font-mono);
    font-size: 0.9em;
    background: var(--panel-2);
    border: 1px solid var(--line);
    border-radius: 5px;
    padding: 0 4px;
  }

  .spacer {
    flex: 1;
  }

  .tools {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
  }

  .tools > button {
    background: none;
    border: 1px solid var(--line);
    border-radius: 7px;
    padding: 1px 8px;
    font-size: 12px;
    color: var(--muted);
    cursor: pointer;
  }

  .tools > button.on {
    color: var(--ink);
    border-color: var(--accent-strong);
    background: color-mix(in srgb, var(--accent-strong) 14%, transparent);
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
    padding: 1px 9px;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 600;
    color: var(--muted);
    cursor: pointer;
  }

  .seg button.on {
    background: var(--accent);
    color: var(--accent-ink);
  }

  .ghost {
    background: none;
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 2px 9px;
    font-size: 12px;
    color: var(--muted);
    cursor: pointer;
  }

  .ghost:hover {
    color: var(--ink);
    border-color: var(--muted);
  }

  .plot {
    position: relative;
    flex: 1;
    min-height: 0;
    border: 1px solid var(--line);
    border-radius: 10px;
    overflow: hidden;
    background: var(--panel);
  }

  canvas {
    display: block;
    touch-action: none;
    cursor: crosshair;
  }

  canvas:active {
    cursor: grabbing;
  }

  .tip {
    position: absolute;
    pointer-events: none;
    min-width: 120px;
    background: var(--panel-2);
    border: 1px solid var(--line);
    border-radius: 8px;
    padding: 6px 9px;
    font-size: 12px;
    color: var(--ink);
    font-variant-numeric: tabular-nums;
    box-shadow: 0 4px 16px rgb(0 0 0 / 0.18);
  }

  .tip-head {
    color: var(--muted);
    margin-bottom: 3px;
  }

  .tip-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }
</style>
