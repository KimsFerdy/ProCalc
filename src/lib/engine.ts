import { invoke } from "@tauri-apps/api/core";

export type AngleMode = "radians" | "degrees";

export interface Settings {
  digits: number;
  angle: AngleMode;
}

export interface DecimalAnswer {
  latex: string;
  text: string;
  is_exact: boolean;
}

export interface Answer {
  /** Exact or simplified result: \frac{1}{3}, 2\sqrt{2}, 3x+1. */
  exact_latex: string | null;
  /** Absent when the result still contains variables. */
  decimal: DecimalAnswer | null;
  symbolic: boolean;
  /** Set for definitions (`x := 5`, `f(x) := x^2`): the name defined, as LaTeX. */
  defines?: string | null;
  /** Worked solutions, one per method, for the Steps view. */
  methods?: Method[];
}

export interface Method {
  name: string;
  steps: Step[];
}

export interface Step {
  /** Maths for this step (may be empty). */
  latex: string;
  /** Plain-text explanation with $…$ around inline maths. */
  note: string;
}

/** History saved before v0.2 stored the decimal flat on the answer. */
export function migrateAnswer(a: any): Answer | null {
  if (!a || typeof a !== "object") return null;
  if ("decimal_text" in a) {
    return {
      exact_latex: a.exact_latex ?? null,
      decimal: { latex: a.decimal_latex, text: a.decimal_text, is_exact: !!a.decimal_is_exact },
      symbolic: false,
    };
  }
  return a as Answer;
}

export interface EngineError {
  message: string;
  position: number | null;
}

export type Outcome = { ok: true; answer: Answer } | { ok: false; error: EngineError };

/** What a Paper line draws on the graph. */
export type Curve = { label: string } & (
  | {
      kind: "function";
      /** Evenly spaced samples across the view's x-range; null where undefined. */
      ys: (number | null)[];
      /** The interval of a definite integral, shaded. */
      shade: [number, number] | null;
    }
  | { kind: "path"; points: ([number, number] | null)[] }
  | {
      kind: "implicit";
      nx: number;
      ny: number;
      /** F(x, y) at (nx+1)×(ny+1) grid corners across the view, row by row from the bottom. */
      values: (number | null)[];
      /** For an inequality: shade where F > 0; true when strict (dashed edge). */
      region: boolean | null;
    }
  | { kind: "tangent"; x0: number; y0: number; slope: number }
  | {
      kind: "surface";
      nx: number;
      ny: number;
      x: [number, number];
      y: [number, number];
      /** z at (nx+1)×(ny+1) grid corners, row by row from y[0]. */
      zs: (number | null)[];
      /** Axis letters: [x, y, z]. */
      axes: [string, string, string];
    }
);

export interface View {
  xMin: number;
  xMax: number;
  yMin: number;
  yMax: number;
  /** Size on screen in pixels: sets how finely curves are sampled. */
  width: number;
  height: number;
}

/** Samples each Paper line over the view (null for lines that draw nothing). */
export async function plotSheet(lines: string[], settings: Settings, view: View): Promise<(Curve | null)[]> {
  try {
    return await invoke<(Curve | null)[]>("plot_sheet", { lines, settings, view });
  } catch {
    return lines.map(() => null);
  }
}

/** Evaluates the Paper's maths lines in order; definitions carry down the sheet. */
export async function evaluateSheet(lines: string[], settings: Settings): Promise<Outcome[]> {
  try {
    const out = await invoke<({ Ok: Answer } | { Err: EngineError })[]>("evaluate_sheet", { lines, settings });
    return out.map((r) => ("Ok" in r ? { ok: true, answer: r.Ok } : { ok: false, error: r.Err }));
  } catch (e) {
    const error = { message: String(e), position: null };
    return lines.map(() => ({ ok: false, error }));
  }
}

// ---------- Geometry workspace ----------

export type FigureKind = "triangle" | "rectangle" | "square" | "polygon" | "circle";

/** A figure as drawn, with what was typed for each side (corner i to i+1) and angle. */
export interface FigureInput {
  kind: FigureKind;
  points: [number, number][];
  sides: (string | null)[];
  angles: (string | null)[];
  circle: { which: "r" | "d" | "C" | "A"; value: string } | null;
}

export interface FigurePart {
  name: string;
  status: "given" | "found" | "fixed" | "unknown";
  latex: string | null;
  text: string | null;
  value: number | null;
}

export interface FigureQuantity {
  id: string;
  name: string;
  /** 1 a length, 2 an area, 0 an angle or a count. */
  dim: number;
  latex: string;
  text: string;
  value: number;
  formula: string;
  steps: Step[];
}

export interface FigureResult {
  status: "solved" | "missing" | "conflict" | "error";
  message: string;
  points: [number, number][];
  sides: FigurePart[];
  angles: FigurePart[];
  quantities: FigureQuantity[];
  radius: number | null;
}

export async function solveFigure(figure: FigureInput, settings: Settings): Promise<FigureResult> {
  try {
    return await invoke<FigureResult>("solve_figure", { figure, settings });
  } catch (e) {
    return { status: "error", message: String(e), points: figure.points, sides: [], angles: [], quantities: [], radius: null };
  }
}

/** `defs`: earlier definition lines, oldest first, so later lines can use them. */
export async function evaluate(input: string, defs: string[], settings: Settings): Promise<Outcome> {
  try {
    const answer = await invoke<Answer>("evaluate", { input, defs, settings });
    return { ok: true, answer };
  } catch (e) {
    const error =
      typeof e === "object" && e !== null && "message" in e
        ? (e as EngineError)
        : { message: String(e), position: null };
    return { ok: false, error };
  }
}
