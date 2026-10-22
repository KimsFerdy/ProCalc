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

export async function evaluate(input: string, settings: Settings): Promise<Outcome> {
  try {
    const answer = await invoke<Answer>("evaluate", { input, settings });
    return { ok: true, answer };
  } catch (e) {
    const error =
      typeof e === "object" && e !== null && "message" in e
        ? (e as EngineError)
        : { message: String(e), position: null };
    return { ok: false, error };
  }
}
