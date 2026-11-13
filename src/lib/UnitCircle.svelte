<script lang="ts">
  // The unit-circle workspace: drag the angle round the circle and see sin, cos and tan as
  // lengths, their exact values at the standard angles, the angle in degrees and radians side
  // by side, and the matching points on the sine and cosine graphs.
  import { onMount } from "svelte";
  import { evaluateSheet, type Settings } from "$lib/engine";
  import type { UnitCircleState } from "$lib/project";

  let {
    settings,
    render,
    onchange,
  }: {
    settings: Settings;
    render: (latex: string) => string;
    /** The angle or a setting changed (worth saving). */
    onchange?: () => void;
  } = $props();

  const STORE_KEY = "procalc.unitcircle.v1";
  /** The standard angles: multiples of 30° and 45°, where sin, cos and tan are exact. */
  const STANDARD = [0, 30, 45, 60, 90, 120, 135, 150, 180, 210, 225, 240, 270, 300, 315, 330];
  /** How close (in degrees) a drag must come to a standard angle to snap onto it. */
  const SNAP_WITHIN = 4;
  const TAU = Math.PI * 2;

  let deg = $state(30);
  let snap = $state(true);
  let typed = $state("");
  let typeError = $state<string | null>(null);
  /** Exact values from the engine: LaTeX, or null when there's no exact form. */
  let exact = $state<{ sin: string | null; cos: string | null; tan: string | null; tanUndefined: boolean }>({
    sin: null,
    cos: null,
    tan: null,
    tanUndefined: false,
  });
  let dragging: "circle" | "wave" | null = null;
  let evalSeq = 0;
  let evalTimer: ReturnType<typeof setTimeout> | undefined;

  // ---------- State and saving ----------

  export function getState(): UnitCircleState {
    return { deg, snap };
  }

  export function setState(s: UnitCircleState | null) {
    deg = s ? normalize(s.deg) : 30;
    snap = s ? s.snap : true;
    changed();
  }

  function changed() {
    try {
      localStorage.setItem(STORE_KEY, JSON.stringify({ deg, snap }));
    } catch {
      /* storage unavailable: the angle just won't persist */
    }
    onchange?.();
  }

  onMount(() => {
    try {
      const saved = JSON.parse(localStorage.getItem(STORE_KEY) ?? "null");
      if (Number.isFinite(saved?.deg)) deg = normalize(saved.deg);
      if (typeof saved?.snap === "boolean") snap = saved.snap;
    } catch {
      /* first run, or storage unavailable */
    }
  });

  // ---------- The angle ----------

  function normalize(d: number): number {
    const r = ((d % 360) + 360) % 360;
    // Tidy floating-point dust so 29.999999 reads as 30.
    const rounded = Math.round(r * 1e6) / 1e6;
    return rounded >= 360 ? 0 : rounded;
  }

  function setAngle(d: number, fromDrag = false) {
    let next = normalize(d);
    if (fromDrag && snap) {
      for (const s of [...STANDARD, 360]) {
        if (Math.abs(next - s) <= SNAP_WITHIN) {
          next = s % 360;
          break;
        }
      }
      if (!STANDARD.includes(next)) next = Math.round(next);
    }
    if (next === deg) return;
    deg = next;
    changed();
  }

  const rad = $derived((deg * Math.PI) / 180);
  const sinV = $derived(clean(Math.sin(rad)));
  const cosV = $derived(clean(Math.cos(rad)));
  const tanUndefined = $derived(Math.abs(cosV) < 1e-12);
  const tanV = $derived(tanUndefined ? NaN : clean(Math.tan(rad)));
  const isStandard = $derived(STANDARD.includes(deg));

  /** Rounds away the 1e-17 that Math.sin(π) leaves behind. */
  function clean(v: number): number {
    return Math.abs(v) < 1e-12 ? 0 : v;
  }

  function gcd(a: number, b: number): number {
    return b ? gcd(b, a % b) : Math.abs(a);
  }

  /** The angle in radians as an exact multiple of π when it's a multiple of 15°. */
  const radLatex = $derived.by(() => {
    if (!Number.isInteger(deg) || deg % 15 !== 0) return null;
    const k = deg / 15;
    if (k === 0) return "0";
    const g = gcd(k, 12);
    const num = k / g;
    const den = 12 / g;
    const top = num === 1 ? "\\pi" : `${num}\\pi`;
    return den === 1 ? top : `\\frac{${top}}{${den}}`;
  });

  const quadrant = $derived.by(() => {
    if (deg % 90 === 0) return null;
    return Math.floor(deg / 90) + 1;
  });

  /** The acute angle to the x-axis, which gives the size of sin, cos and tan. */
  const reference = $derived.by(() => {
    if (deg <= 90) return deg;
    if (deg <= 180) return 180 - deg;
    if (deg <= 270) return deg - 180;
    return 360 - deg;
  });

  const signs = $derived.by(() => {
    const s = (v: number) => (v > 0 ? "+" : v < 0 ? "−" : "0");
    return { sin: s(sinV), cos: s(cosV), tan: tanUndefined ? "∞" : s(tanV) };
  });

  function fmt(v: number, digits = 6): string {
    if (!Number.isFinite(v)) return "—";
    const t = v.toFixed(digits).replace(/\.?0+$/, "");
    return t === "-0" ? "0" : t.replace("-", "−");
  }

  function degText(d: number): string {
    return `${fmt(d, 2)}°`;
  }

  // Exact values from the engine, a moment after the angle settles.
  $effect(() => {
    const d = deg;
    clearTimeout(evalTimer);
    evalTimer = setTimeout(() => refreshExact(d), 60);
  });

  async function refreshExact(d: number) {
    const seq = ++evalSeq;
    const arg = `\\left(${d}^{\\circ}\\right)`;
    const out = await evaluateSheet([`\\sin${arg}`, `\\cos${arg}`, `\\tan${arg}`], { digits: 16, angle: "degrees" });
    if (seq !== evalSeq) return;
    const pick = (k: number) => {
      const r = out[k];
      return r?.ok && r.answer.exact_latex ? r.answer.exact_latex : null;
    };
    exact = { sin: pick(0), cos: pick(1), tan: pick(2), tanUndefined: !out[2]?.ok };
  }

  // ---------- Typing an angle ----------

  /** Reads "150", "150°", "5π/6", "5pi/6" or "2.1": degrees or radians by the mode, unless it says. */
  async function applyTyped() {
    const text = typed.trim();
    if (!text) return;
    const saysDegrees = /°|\\circ|deg/i.test(text);
    const saysPi = /π|pi|\\pi/i.test(text);
    const asDegrees = saysDegrees || (!saysPi && settings.angle === "degrees");
    const expr = text.replace(/°|\\circ|deg/gi, "").replace(/π/g, "pi");
    const [r] = await evaluateSheet([expr], { digits: 16, angle: "radians" });
    const value = r?.ok && r.answer.decimal ? Number(r.answer.decimal.text.replace(/\s/g, "")) : NaN;
    if (!r?.ok || !Number.isFinite(value)) {
      typeError = r && !r.ok ? r.error.message : "That isn't an angle ProCalc can read.";
      return;
    }
    typeError = null;
    setAngle(asDegrees ? value : (value * 180) / Math.PI);
    typed = "";
  }

  // ---------- Drawing: the circle ----------

  /** Circle panel: unit circle of radius R px, centred in the view box. */
  const R = 150;
  const VB = { x: -235, y: -205, w: 470, h: 410 };
  /** Where the tangent line's segment is drawn up to before it leaves the panel. */
  const TAN_LIMIT = 1.3;

  const px = (x: number) => x * R;
  const py = (y: number) => -y * R;

  const P = $derived({ x: px(cosV), y: py(sinV) });
  const tanShown = $derived(tanUndefined ? null : Math.max(-TAN_LIMIT, Math.min(TAN_LIMIT, tanV)));

  /** The angle arc from the x-axis to θ, counter-clockwise. */
  const arcPath = $derived.by(() => {
    const r = 34;
    if (deg === 0) return "";
    const end = { x: r * Math.cos(rad), y: -r * Math.sin(rad) };
    const large = deg > 180 ? 1 : 0;
    return `M ${r} 0 A ${r} ${r} 0 ${large} 0 ${end.x} ${end.y}`;
  });

  const arcLabel = $derived({ x: 54 * Math.cos(rad / 2), y: -54 * Math.sin(rad / 2) });

  function standardLabel(d: number): string {
    if (settings.angle === "degrees") return `${d}°`;
    const k = d / 15;
    if (k === 0) return "0";
    const g = gcd(k, 12);
    const num = k / g;
    const den = 12 / g;
    const top = num === 1 ? "π" : `${num}π`;
    return den === 1 ? top : `${top}/${den}`;
  }

  let svg = $state<SVGSVGElement | undefined>();

  function angleAt(e: PointerEvent): number {
    const m = svg!.getScreenCTM();
    if (!m) return deg;
    const pt = new DOMPoint(e.clientX, e.clientY).matrixTransform(m.inverse());
    return (Math.atan2(-pt.y, pt.x) * 180) / Math.PI;
  }

  function circleDown(e: PointerEvent) {
    dragging = "circle";
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    setAngle(angleAt(e), true);
  }

  function circleMove(e: PointerEvent) {
    if (dragging === "circle") setAngle(angleAt(e), true);
  }

  function stopDrag() {
    dragging = null;
  }

  function onKey(e: KeyboardEvent) {
    const step = e.shiftKey ? 15 : 1;
    if (e.key === "ArrowUp" || e.key === "ArrowLeft") setAngle(Math.round(deg) + step);
    else if (e.key === "ArrowDown" || e.key === "ArrowRight") setAngle(Math.round(deg) - step);
    else return;
    e.preventDefault();
  }

  // ---------- Drawing: the waves ----------

  const W = { w: 760, h: 210, left: 46, right: 16, top: 16, bottom: 30 };
  const wx = (d: number) => W.left + (d / 360) * (W.w - W.left - W.right);
  const wy = (v: number) => W.top + ((1.25 - v) / 2.5) * (W.h - W.top - W.bottom);

  function wavePath(f: (r: number) => number): string {
    let d = "";
    for (let i = 0; i <= 180; i++) {
      const a = i * 2;
      d += `${i ? "L" : "M"} ${wx(a).toFixed(1)} ${wy(f((a * Math.PI) / 180)).toFixed(1)} `;
    }
    return d;
  }
  const sinPath = wavePath(Math.sin);
  const cosPath = wavePath(Math.cos);

  function waveTick(d: number): string {
    if (settings.angle === "degrees") return `${d}°`;
    return ["0", "π/2", "π", "3π/2", "2π"][d / 90];
  }

  let waveSvg = $state<SVGSVGElement | undefined>();

  function waveAngle(e: PointerEvent): number {
    const m = waveSvg!.getScreenCTM();
    if (!m) return deg;
    const pt = new DOMPoint(e.clientX, e.clientY).matrixTransform(m.inverse());
    const d = ((pt.x - W.left) / (W.w - W.left - W.right)) * 360;
    return Math.max(0, Math.min(359.99, d));
  }

  function waveDown(e: PointerEvent) {
    dragging = "wave";
    (e.currentTarget as Element).setPointerCapture(e.pointerId);
    setAngle(waveAngle(e), true);
  }

  function waveMove(e: PointerEvent) {
    if (dragging === "wave") setAngle(waveAngle(e), true);
  }

  function toggleSnap() {
    snap = !snap;
    changed();
  }
</script>

<section class="uc" aria-label="Unit circle">
  <div class="bar">
    <span class="title">Unit circle</span>
    <div class="spacer"></div>
    <button class="ghost" class:on={snap} onclick={toggleSnap} title="While dragging, stick to 0°, 30°, 45°, 60°, 90° and so on">
      Snap to standard angles
    </button>
  </div>

  <div class="grid">
    <!-- The circle -->
    <div class="card circle-card">
      <svg
        bind:this={svg}
        viewBox="{VB.x} {VB.y} {VB.w} {VB.h}"
        class="circle"
        role="slider"
        tabindex="0"
        aria-label="Angle θ"
        aria-valuemin="0"
        aria-valuemax="360"
        aria-valuenow={deg}
        aria-valuetext="{degText(deg)}"
        onpointerdown={circleDown}
        onpointermove={circleMove}
        onpointerup={stopDrag}
        onpointercancel={stopDrag}
        onkeydown={onKey}
      >
        <!-- Axes -->
        <line class="axis" x1={-R - 40} y1="0" x2={R + 40} y2="0" />
        <line class="axis" x1="0" y1={-R - 30} x2="0" y2={R + 30} />
        <text class="axis-label" x={R + 44} y="4">x</text>
        <text class="axis-label" x="6" y={-R - 32}>y</text>

        <!-- The circle and its standard angles -->
        <circle class="ring" cx="0" cy="0" r={R} />
        {#each STANDARD as s}
          {@const a = (s * Math.PI) / 180}
          <circle class="tick" class:hit={s === deg} cx={px(Math.cos(a))} cy={py(Math.sin(a))} r="3" />
          <text
            class="tick-label"
            class:hit={s === deg}
            x={(R + 22) * Math.cos(a)}
            y={-(R + 22) * Math.sin(a) + 4}
            text-anchor="middle">{standardLabel(s)}</text
          >
        {/each}

        <!-- tan on the tangent line x = 1 -->
        <line class="tangent-line" x1={R} y1={-R * 1.3} x2={R} y2={R * 1.3} />
        {#if tanShown !== null}
          <line class="ray" x1="0" y1="0" x2={px(1)} y2={py(tanShown)} />
          <line class="seg tan" x1={R} y1="0" x2={R} y2={py(tanShown)} />
          <text class="seg-label tan" x={R + 8} y={py(tanShown / 2) + 4}>tan</text>
        {/if}

        <!-- cos along the x-axis, sin straight up to P -->
        <line class="seg cos" x1="0" y1="0" x2={P.x} y2="0" />
        <line class="seg sin" x1={P.x} y1="0" x2={P.x} y2={P.y} />
        {#if Math.abs(cosV) > 0.08}
          <text class="seg-label cos" x={P.x / 2} y={sinV >= 0 ? 18 : -8} text-anchor="middle">cos</text>
        {/if}
        {#if Math.abs(sinV) > 0.08}
          <text class="seg-label sin" x={P.x + (cosV >= 0 ? 8 : -8)} y={P.y / 2 + 4} text-anchor={cosV >= 0 ? "start" : "end"}>sin</text>
        {/if}

        <!-- The radius and the angle -->
        <line class="radius" x1="0" y1="0" x2={P.x} y2={P.y} />
        <path class="arc" d={arcPath} />
        {#if deg > 8}
          <text class="arc-label" x={arcLabel.x} y={arcLabel.y + 5} text-anchor="middle">θ</text>
        {/if}
        <circle class="origin" cx="0" cy="0" r="3" />
        <circle class="point-halo" cx={P.x} cy={P.y} r="16" />
        <circle class="point" cx={P.x} cy={P.y} r="8" />
        <!-- P sits just inside the circle, clear of the angle labels outside it. -->
        <text class="point-label" x={(R - 26) * Math.cos(rad)} y={-(R - 26) * Math.sin(rad) + 5} text-anchor="middle">P</text>
      </svg>
      <p class="hint">Drag P round the circle, or use the arrow keys (Shift for 15° steps).</p>
    </div>

    <!-- The inspector -->
    <div class="side">
      <div class="card angle-card">
        <div class="label">Angle θ</div>
        <div class="angle-row">
          <div class="angle-big" class:primary={settings.angle === "degrees"}>
            <span class="unit">degrees</span>
            <span class="num">{degText(deg)}</span>
          </div>
          <div class="angle-big" class:primary={settings.angle === "radians"}>
            <span class="unit">radians</span>
            <span class="num">
              {#if radLatex}{@html render(radLatex)}{:else}{fmt(rad, 4)}{/if}
            </span>
          </div>
        </div>
        <form class="type-angle" onsubmit={(e) => (e.preventDefault(), applyTyped())}>
          <input
            bind:value={typed}
            placeholder={settings.angle === "degrees" ? "Type an angle: 150, 5π/6, 2.1 rad…" : "Type an angle: 5π/6, 150°, 2.1…"}
            aria-label="Type an angle"
          />
          <button type="submit" class="primary">Go</button>
        </form>
        {#if typeError}<p class="error">{typeError}</p>{/if}
        <div class="facts">
          {#if quadrant}
            <span class="chip">Quadrant {["I", "II", "III", "IV"][quadrant - 1]}</span>
          {:else}
            <span class="chip">On an axis</span>
          {/if}
          <span class="chip">Reference angle {degText(reference)}</span>
        </div>
      </div>

      <div class="card values">
        {#each [{ key: "sin", v: sinV, ex: exact.sin, sign: signs.sin }, { key: "cos", v: cosV, ex: exact.cos, sign: signs.cos }, { key: "tan", v: tanV, ex: exact.tan, sign: signs.tan }] as row}
          <div class="value-row {row.key}">
            <span class="dot" aria-hidden="true"></span>
            <span class="fn">{row.key} θ</span>
            <span class="val">
              {#if row.key === "tan" && tanUndefined}
                <span class="undef">undefined</span>
                <span class="why">cos θ = 0</span>
              {:else if row.ex && isStandard}
                {@const plainInteger = /^-?\d+$/.test(row.ex)}
                <span class="exact">{@html render(row.ex)}</span>
                {#if !plainInteger}
                  <span class="dec">{row.ex.includes("\\sqrt") ? "≈" : "="} {fmt(row.v)}</span>
                {/if}
              {:else}
                <span class="dec big">≈ {fmt(row.v)}</span>
              {/if}
            </span>
          </div>
        {/each}
        <p class="identity">
          {@html render(`\\sin^2\\theta+\\cos^2\\theta=${fmt(sinV * sinV + cosV * cosV, 6) === "1" ? "1" : fmt(sinV * sinV + cosV * cosV, 6)}`)}
        </p>
      </div>

      <div class="card coords">
        <div class="label">The point P</div>
        <div class="p-coords">
          {@html render(`P=\\left(\\cos\\theta,\\ \\sin\\theta\\right)=\\left(${exact.cos && isStandard ? exact.cos : fmt(cosV, 4).replace("−", "-")},\\ ${exact.sin && isStandard ? exact.sin : fmt(sinV, 4).replace("−", "-")}\\right)`)}
        </div>
      </div>
    </div>

    <!-- The waves -->
    <div class="card wave-card">
      <div class="wave-head">
        <span class="label">sin and cos over one turn</span>
        <span class="legend"><span class="dot sin"></span>sin θ <span class="dot cos"></span>cos θ</span>
      </div>
      <svg
        bind:this={waveSvg}
        viewBox="0 0 {W.w} {W.h}"
        class="wave"
        role="img"
        aria-label="The sine and cosine graphs with θ marked"
        onpointerdown={waveDown}
        onpointermove={waveMove}
        onpointerup={stopDrag}
        onpointercancel={stopDrag}
      >
        {#each [-1, 0, 1] as v}
          <line class={v === 0 ? "axis" : "gridline"} x1={W.left} y1={wy(v)} x2={W.w - W.right} y2={wy(v)} />
          <text class="tick-label" x={W.left - 8} y={wy(v) + 4} text-anchor="end">{v === -1 ? "−1" : v}</text>
        {/each}
        {#each [0, 90, 180, 270, 360] as d}
          <line class="gridline" x1={wx(d)} y1={W.top} x2={wx(d)} y2={W.h - W.bottom} />
          <text class="tick-label" x={wx(d)} y={W.h - 8} text-anchor="middle">{waveTick(d)}</text>
        {/each}
        <path class="curve cos" d={cosPath} />
        <path class="curve sin" d={sinPath} />
        <line class="marker" x1={wx(deg)} y1={W.top} x2={wx(deg)} y2={W.h - W.bottom} />
        <line class="drop sin" x1={wx(deg)} y1={wy(0)} x2={wx(deg)} y2={wy(sinV)} />
        <circle class="wave-point cos" cx={wx(deg)} cy={wy(cosV)} r="6" />
        <circle class="wave-point sin" cx={wx(deg)} cy={wy(sinV)} r="6" />
      </svg>
    </div>
  </div>
</section>

<style>
  .uc {
    --sin: var(--br-1);
    --cos: var(--br-0);
    --tan: var(--br-3);
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
    max-width: 1200px;
    margin: 0 auto;
  }

  .title {
    font-weight: 700;
    font-size: 20px;
    letter-spacing: -0.2px;
  }

  .spacer {
    flex: 1;
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
  }

  .ghost:hover {
    color: var(--ink);
  }

  .ghost.on {
    background: var(--accent);
    color: var(--accent-ink);
  }

  .grid {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(300px, 380px);
    grid-template-rows: auto auto;
    gap: 14px;
    padding: 6px 20px 24px;
    width: 100%;
    max-width: 1200px;
    margin: 0 auto;
    align-content: start;
  }

  .card {
    background: var(--panel);
    border-radius: var(--radius);
    padding: 16px 18px;
  }

  .label {
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.6px;
    text-transform: uppercase;
    color: var(--muted);
  }

  /* ---------- The circle ---------- */

  .circle-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    padding: 10px;
  }

  .circle {
    width: 100%;
    max-height: min(470px, 52vh);
    touch-action: none;
    cursor: grab;
    outline: none;
    border-radius: var(--radius-sm);
  }

  .circle:focus-visible {
    box-shadow: inset 0 0 0 2px var(--accent-strong);
  }

  .circle:active {
    cursor: grabbing;
  }

  .hint {
    margin: 2px 0 4px;
    font-size: 12.5px;
    color: var(--muted);
  }

  .axis {
    stroke: var(--muted);
    stroke-width: 1;
    opacity: 0.6;
  }

  .gridline {
    stroke: var(--line);
    stroke-width: 1;
  }

  .axis-label {
    fill: var(--muted);
    font-size: 13px;
    font-style: italic;
  }

  .ring {
    fill: color-mix(in srgb, var(--accent) 5%, transparent);
    stroke: var(--ink);
    stroke-width: 1.6;
    opacity: 0.85;
  }

  .tick {
    fill: var(--muted);
  }

  .tick.hit {
    fill: var(--accent-strong);
  }

  .tick-label {
    fill: var(--muted);
    font-size: 11.5px;
    font-variant-numeric: tabular-nums;
    user-select: none;
  }

  .tick-label.hit {
    fill: var(--ink);
    font-weight: 700;
  }

  .tangent-line {
    stroke: var(--line);
    stroke-width: 1;
    stroke-dasharray: 4 4;
  }

  .ray {
    stroke: var(--muted);
    stroke-width: 1;
    stroke-dasharray: 3 4;
  }

  .seg {
    stroke-width: 4;
    stroke-linecap: round;
  }

  .seg.sin {
    stroke: var(--sin);
  }

  .seg.cos {
    stroke: var(--cos);
  }

  .seg.tan {
    stroke: var(--tan);
  }

  .seg-label {
    font-size: 13px;
    font-weight: 700;
    user-select: none;
  }

  .seg-label.sin {
    fill: var(--sin);
  }

  .seg-label.cos {
    fill: var(--cos);
  }

  .seg-label.tan {
    fill: var(--tan);
  }

  .radius {
    stroke: var(--ink);
    stroke-width: 2;
  }

  .arc {
    fill: none;
    stroke: var(--accent-strong);
    stroke-width: 2.5;
  }

  .arc-label {
    fill: var(--accent-strong);
    font-size: 15px;
    font-style: italic;
    font-weight: 700;
  }

  .origin {
    fill: var(--ink);
  }

  .point-halo {
    fill: color-mix(in srgb, var(--accent) 30%, transparent);
  }

  .point {
    fill: var(--accent);
    stroke: var(--accent-ink);
    stroke-width: 2;
  }

  .point-label {
    fill: var(--ink);
    font-size: 15px;
    font-weight: 700;
    font-style: italic;
    user-select: none;
  }

  /* ---------- The inspector ---------- */

  .side {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-width: 0;
  }

  .angle-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
    margin: 10px 0 12px;
  }

  .angle-big {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 10px 12px;
    border-radius: var(--radius-sm);
    background: var(--panel-2);
  }

  .angle-big.primary {
    background: var(--accent);
    color: var(--accent-ink);
  }

  .angle-big .unit {
    font-size: 11.5px;
    font-weight: 600;
    opacity: 0.7;
  }

  .angle-big .num {
    font-size: 26px;
    font-weight: 700;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .type-angle {
    display: flex;
    gap: 6px;
  }

  .type-angle input {
    flex: 1;
    min-width: 0;
    border: none;
    border-radius: 999px;
    padding: 8px 14px;
    background: var(--panel-2);
    color: var(--ink);
    font: inherit;
    font-size: 13.5px;
    outline: none;
  }

  .type-angle input:focus-visible {
    box-shadow: 0 0 0 2px var(--accent-strong);
  }

  .primary {
    border: none;
    border-radius: 999px;
    padding: 8px 16px;
    background: var(--ink);
    color: var(--bg);
    font: inherit;
    font-size: 13.5px;
    font-weight: 700;
    cursor: pointer;
  }

  .error {
    margin: 8px 0 0;
    font-size: 12.5px;
    color: var(--err);
  }

  .facts {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 12px;
  }

  .chip {
    padding: 4px 10px;
    border-radius: 999px;
    background: var(--panel-2);
    font-size: 12.5px;
    font-weight: 600;
    color: var(--muted);
  }

  .values {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .value-row {
    display: grid;
    grid-template-columns: 12px 56px minmax(0, 1fr);
    align-items: center;
    gap: 10px;
    padding: 8px 0;
    border-bottom: 1px solid var(--line);
  }

  .dot {
    display: inline-block;
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }

  .value-row.sin .dot,
  .dot.sin {
    background: var(--sin);
  }

  .value-row.cos .dot,
  .dot.cos {
    background: var(--cos);
  }

  .value-row.tan .dot {
    background: var(--tan);
  }

  .fn {
    font-weight: 600;
    color: var(--muted);
    font-style: italic;
  }

  .val {
    display: flex;
    align-items: baseline;
    justify-content: flex-end;
    gap: 10px;
    white-space: nowrap;
    overflow-x: auto;
    overflow-y: hidden;
  }

  .exact {
    font-size: 21px;
  }

  .dec {
    font-size: 14px;
    color: var(--accent-strong);
    font-variant-numeric: tabular-nums;
  }

  .dec.big {
    font-size: 18px;
    color: var(--ink);
  }

  .undef {
    font-weight: 700;
    color: var(--tan);
  }

  .why {
    font-size: 12.5px;
    color: var(--muted);
  }

  .identity {
    margin: 8px 0 0;
    font-size: 14px;
    color: var(--muted);
    text-align: center;
  }

  .p-coords {
    margin-top: 8px;
    font-size: 18px;
    overflow-x: auto;
    overflow-y: hidden;
    white-space: nowrap;
  }

  /* ---------- The waves ---------- */

  .wave-card {
    grid-column: 1 / -1;
  }

  .wave-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 6px;
  }

  .legend {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
    color: var(--muted);
    font-style: italic;
  }

  .legend .dot.cos {
    margin-left: 10px;
  }

  .wave {
    width: 100%;
    height: auto;
    touch-action: none;
    cursor: ew-resize;
  }

  .curve {
    fill: none;
    stroke-width: 3;
    stroke-linecap: round;
  }

  .curve.sin {
    stroke: var(--sin);
  }

  .curve.cos {
    stroke: var(--cos);
  }

  .marker {
    stroke: var(--accent-strong);
    stroke-width: 1.5;
    stroke-dasharray: 4 4;
  }

  .drop.sin {
    stroke: var(--sin);
    stroke-width: 4;
    stroke-linecap: round;
    opacity: 0.6;
  }

  .wave-point {
    stroke: var(--panel);
    stroke-width: 2;
  }

  .wave-point.sin {
    fill: var(--sin);
  }

  .wave-point.cos {
    fill: var(--cos);
  }

  @media (max-width: 760px) {
    .grid {
      grid-template-columns: minmax(0, 1fr);
      padding: 6px 12px 20px;
    }
    .bar {
      padding: 12px 12px 8px;
    }
  }
</style>
