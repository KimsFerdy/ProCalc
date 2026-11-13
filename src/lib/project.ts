// Project files: one file per project, with an extension per subject (.pcmath, .pcphysics, …)
// so the name says what's inside. All subjects share this format, which records the subject
// too, so opening one that isn't available yet can explain what it is.

import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { FigureInput } from "$lib/engine";

/** Bumped when the format changes in a way older versions can't read. */
export const FORMAT_VERSION = 1;

export interface Subject {
  id: string;
  name: string;
  extension: string;
  /** Can this version of ProCalc open it? */
  ready: boolean;
}

export const SUBJECTS: Subject[] = [
  { id: "math", name: "Math", extension: "pcmath", ready: true },
  { id: "physics", name: "Physics", extension: "pcphysics", ready: false },
  { id: "chemistry", name: "Chemistry", extension: "pcchem", ready: false },
  { id: "engineering", name: "Engineering", extension: "pceng", ready: false },
  { id: "finance", name: "Finance", extension: "pcfin", ready: false },
];

export interface PaperLine {
  kind: "math" | "text";
  src: string;
}

/** What a Paper holds: its lines and how its view was left. */
export interface PaperState {
  lines: PaperLine[];
  showSteps: boolean;
  showGraph: boolean;
  graphShare: number;
  view: { xMin: number; xMax: number; yMin: number; yMax: number };
}

export interface ProjectSettings {
  digits: number;
  angle: "degrees" | "radians";
  lengthUnit: string;
}

export interface Project {
  procalc: number;
  subject: string;
  saved: string;
  settings: ProjectSettings;
  /** The workspace that was open: the Paper or a drawing one such as geometry. */
  workspace: Workspace;
  paper: PaperState;
  /** The geometry workspace's figure, if one was drawn. */
  geometry: FigureInput | null;
  /** The unit circle's angle (missing in files from before 0.3.1). */
  unitCircle: UnitCircleState | null;
}

/** The unit-circle workspace: its angle and whether dragging snaps to the standard angles. */
export interface UnitCircleState {
  /** The angle in degrees, 0 ≤ deg < 360. */
  deg: number;
  snap: boolean;
}

/** Math's workspaces. Files from before workspaces existed open on the Paper. */
export type Workspace = "paper" | "geometry" | "unit-circle";

const WORKSPACE_IDS: Workspace[] = ["paper", "geometry", "unit-circle"];

export function makeProject(
  subject: string,
  settings: ProjectSettings,
  workspace: Workspace,
  paper: PaperState,
  geometry: FigureInput | null,
  unitCircle: UnitCircleState | null,
): Project {
  return { procalc: FORMAT_VERSION, subject, saved: new Date().toISOString(), settings, workspace, paper, geometry, unitCircle };
}

const FIGURE_KINDS = ["triangle", "rectangle", "square", "polygon", "circle"];

/** A saved figure, or null when it's missing or damaged. */
function parseFigure(f: any): FigureInput | null {
  if (!f || !FIGURE_KINDS.includes(f.kind) || !Array.isArray(f.points)) return null;
  const points: [number, number][] = f.points
    .filter((p: any) => Array.isArray(p) && Number.isFinite(p[0]) && Number.isFinite(p[1]))
    .map((p: any) => [p[0], p[1]]);
  if (points.length !== f.points.length || points.length < 2) return null;
  const n = f.kind === "circle" ? 0 : points.length;
  const values = (v: any) => Array.from({ length: n }, (_, i) => (Array.isArray(v) && typeof v[i] === "string" ? (v[i] as string) : null));
  const c = f.circle;
  const circle = c && ["r", "d", "C", "A"].includes(c.which) && typeof c.value === "string" ? { which: c.which, value: c.value } : null;
  return { kind: f.kind, points, sides: values(f.sides), angles: values(f.angles), circle };
}

/** The file's name without its folder or extension. */
export function projectName(path: string | null): string {
  if (!path) return "Untitled";
  const file = path.split(/[\\/]/).pop() ?? path;
  return file.replace(/\.[^.]+$/, "");
}

/**
 * Reads and checks a project file. Throws a message meant for the user when the file isn't a
 * ProCalc project, is from a newer version, or is a subject this version can't open yet.
 */
export function parseProject(text: string): Project {
  let data: any;
  try {
    data = JSON.parse(text);
  } catch {
    throw new Error("This file isn't a ProCalc project (it couldn't be read).");
  }
  if (typeof data?.procalc !== "number" || typeof data?.subject !== "string") {
    throw new Error("This file isn't a ProCalc project.");
  }
  if (data.procalc > FORMAT_VERSION) {
    throw new Error("This project was saved by a newer version of ProCalc. Update ProCalc to open it.");
  }
  const subject = SUBJECTS.find((s) => s.id === data.subject);
  if (!subject) {
    throw new Error(`This project is for "${data.subject}", which this version of ProCalc doesn't know.`);
  }
  if (!subject.ready) {
    throw new Error(`This is a ${subject.name} project (.${subject.extension}). ${subject.name} isn't available in this version of ProCalc yet.`);
  }
  const lines = Array.isArray(data.paper?.lines)
    ? data.paper.lines.filter((l: any) => typeof l?.src === "string").map((l: any) => ({ kind: l.kind === "text" ? "text" : "math", src: l.src }))
    : [];
  const view = data.paper?.view;
  const okView = view && ["xMin", "xMax", "yMin", "yMax"].every((k) => Number.isFinite(view[k]));
  return {
    procalc: data.procalc,
    subject: data.subject,
    saved: typeof data.saved === "string" ? data.saved : "",
    settings: {
      digits: Number.isFinite(data.settings?.digits) ? data.settings.digits : 32,
      angle: data.settings?.angle === "degrees" ? "degrees" : "radians",
      lengthUnit: typeof data.settings?.lengthUnit === "string" ? data.settings.lengthUnit : "m",
    },
    workspace: WORKSPACE_IDS.includes(data.workspace) ? data.workspace : "paper",
    geometry: parseFigure(data.geometry),
    unitCircle: Number.isFinite(data.unitCircle?.deg) ? { deg: data.unitCircle.deg, snap: data.unitCircle.snap !== false } : null,
    paper: {
      lines: lines.length ? lines : [{ kind: "math", src: "" }],
      showSteps: data.paper?.showSteps === true,
      showGraph: data.paper?.showGraph === true,
      graphShare: Number.isFinite(data.paper?.graphShare) ? data.paper.graphShare : 58,
      view: okView ? view : { xMin: -10, xMax: 10, yMin: -6, yMax: 6 },
    },
  };
}

/** Asks where to open a project from; null when cancelled. */
export async function chooseOpenPath(): Promise<string | null> {
  const picked = await open({
    multiple: false,
    directory: false,
    filters: [
      { name: "ProCalc projects", extensions: SUBJECTS.map((s) => s.extension) },
      { name: "All files", extensions: ["*"] },
    ],
  });
  return typeof picked === "string" ? picked : null;
}

/** Asks where to save; null when cancelled. Adds the subject's extension if it's missing. */
export async function chooseSavePath(subject: string, current: string | null): Promise<string | null> {
  const ext = SUBJECTS.find((s) => s.id === subject)?.extension ?? "pcmath";
  const picked = await save({
    defaultPath: current ?? `Untitled.${ext}`,
    filters: [{ name: `ProCalc ${SUBJECTS.find((s) => s.id === subject)?.name ?? ""} project`, extensions: [ext] }],
  });
  if (!picked) return null;
  return picked.toLowerCase().endsWith(`.${ext}`) ? picked : `${picked}.${ext}`;
}

export async function readProjectFile(path: string): Promise<Project> {
  return parseProject(await invoke<string>("read_project", { path }));
}

export async function writeProjectFile(path: string, project: Project): Promise<void> {
  await invoke("write_project", { path, contents: JSON.stringify(project, null, 2) });
}

// ---------- Recent projects ----------

const RECENT_KEY = "procalc.recent";
const MAX_RECENT = 8;

export function recentProjects(): string[] {
  try {
    const list = JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]");
    return Array.isArray(list) ? list.filter((p) => typeof p === "string").slice(0, MAX_RECENT) : [];
  } catch {
    return [];
  }
}

export function rememberRecent(path: string): string[] {
  const list = [path, ...recentProjects().filter((p) => p !== path)].slice(0, MAX_RECENT);
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(list));
  } catch {
    /* storage unavailable: the list just won't persist */
  }
  return list;
}

export function forgetRecent(path: string): string[] {
  const list = recentProjects().filter((p) => p !== path);
  try {
    localStorage.setItem(RECENT_KEY, JSON.stringify(list));
  } catch {
    /* storage unavailable */
  }
  return list;
}
