// Turns the engine's steps into a worked chain, the way it's written on paper:
//
//   d/dx sin(x²) = cos(x²) · d/dx(x²)
//                = 2x cos(x²)
//
// Each engine step is a complete statement ("A = B"). When later steps rewrite the same A,
// they continue the chain as "= B" rows; anything else (a side calculation, reading off
// coefficients, an equation being solved) stays a full row of its own.

import type { Step } from "$lib/engine";

export interface Row {
  /** "=" when the row continues the chain above it, "" for a full statement. */
  rel: "=" | "";
  latex: string;
  /** The step's explanation, shown small above the row. */
  note: string;
  /** A side calculation in the middle of a chain (shown indented, so the chain reads on). */
  aside: boolean;
}

/** Splits LaTeX at its top-level "=" signs (not inside braces, \text{…} or subscripts). */
export function splitAtEquals(latex: string): string[] {
  const parts: string[] = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < latex.length; i++) {
    const c = latex[i];
    if (c === "\\") {
      i++; // skip the escaped character (\{, \}, \=)
      continue;
    }
    if (c === "{") depth++;
    else if (c === "}") depth = Math.max(0, depth - 1);
    else if (c === "=" && depth === 0) {
      // "==" or a relation like "≤" written with "=" next to < or > is not a chain link.
      const prev = latex[i - 1];
      const next = latex[i + 1];
      if (prev === "<" || prev === ">" || prev === "!" || next === "=") continue;
      parts.push(latex.slice(start, i));
      start = i + 1;
    }
  }
  parts.push(latex.slice(start));
  return parts.map((p) => p.trim());
}

const SEPARATORS = /\\quad|\\qquad|\\text\{\s*or\s*\}|\\Rightarrow|\\iff|\\implies|,/;

function normalize(latex: string): string {
  return latex.replace(/\\[,;:! ]|\s+/g, "");
}

/** A step that reads as "A = B (= C …)": one expression rewritten, not a list or an equation. */
function chainParts(latex: string): string[] | null {
  const parts = splitAtEquals(latex);
  if (parts.length < 2 || parts.some((p) => !p || SEPARATORS.test(p))) return null;
  return parts;
}

export function workedRows(steps: Step[]): Row[] {
  const parsed = steps.map((s) => (s.latex ? chainParts(s.latex) : null));
  // How often each left-hand side is rewritten: a chain needs one A rewritten more than once.
  const seen = new Map<string, number>();
  for (const p of parsed) if (p) seen.set(normalize(p[0]), (seen.get(normalize(p[0])) ?? 0) + 1);

  const rows: Row[] = [];
  /** Rows that start a chain ("A = B" with "= …" rows to follow). */
  const heads = new Set<Row>();
  let subject: string | null = null;
  steps.forEach((s, k) => {
    const parts = parsed[k];
    const lhs = parts ? normalize(parts[0]) : null;
    // A one-off "A = B = C" stays on one line; only an A rewritten across steps becomes a chain.
    const chains = parts && lhs !== null && ((seen.get(lhs) ?? 0) > 1 || lhs === subject);
    if (!s.latex) {
      if (s.note) rows.push({ rel: "", latex: "", note: s.note, aside: false });
    } else if (parts && chains && lhs === subject) {
      parts.slice(1).forEach((p, i) => rows.push({ rel: "=", latex: p, note: i === 0 ? s.note : "", aside: false }));
    } else if (parts && chains) {
      subject = lhs;
      const head: Row = { rel: "", latex: `${parts[0]}=${parts[1]}`, note: s.note, aside: false };
      heads.add(head);
      rows.push(head);
      parts.slice(2).forEach((p) => rows.push({ rel: "=", latex: p, note: "", aside: false }));
    } else {
      rows.push({ rel: "", latex: s.latex, note: s.note, aside: false });
    }
  });
  // A full row with the chain picking up again after it is a side calculation.
  rows.forEach((r, k) => {
    if (r.rel || !r.latex || heads.has(r)) return;
    const next = rows.slice(k + 1).find((x) => x.latex);
    const before = rows.slice(0, k).some((x) => x.latex);
    if (before && next?.rel === "=") r.aside = true;
  });
  return rows;
}
