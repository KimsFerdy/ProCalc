// Updates, the way OBS does them: a quiet check at startup, a dialog when there's something new
// (install now, remind me later, skip this version), and a Check now button in Settings.
//
// Updates come from the latest GitHub release's latest.json and are verified against the public
// key in tauri.conf.json before anything is installed.

import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";

const PREFS_KEY = "procalc.updates";
const RELEASES_URL = "https://github.com/KimsFerdy/ProCalc/releases";
/** How long after launch the automatic check waits, so it never slows the start. */
const STARTUP_DELAY_MS = 4000;

export type UpdateStatus =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "current" }
  | { kind: "available" }
  | { kind: "downloading"; done: number; total: number | null }
  | { kind: "restarting" }
  | { kind: "error"; message: string };

interface Prefs {
  auto: boolean;
  skipped: string | null;
  lastChecked: number | null;
}

class Updates {
  /** This copy's version, e.g. "0.1.0". */
  version = $state("");
  /** Portable zips can't install updates over themselves; they get a download link instead. */
  portable = $state(false);
  auto = $state(true);
  skipped = $state<string | null>(null);
  lastChecked = $state<number | null>(null);
  status = $state<UpdateStatus>({ kind: "idle" });
  update = $state.raw<Update | null>(null);
  /** Whether the "update available" dialog is showing. */
  prompt = $state(false);

  private started = false;

  /** Reads the saved preferences and, if allowed, checks shortly after launch. Call once. */
  async start() {
    if (this.started) return;
    this.started = true;
    this.loadPrefs();
    try {
      this.version = await getVersion();
      this.portable = await invoke<boolean>("is_portable");
    } catch {
      return; // not running inside the app (e.g. the plain dev server)
    }
    if (this.auto) setTimeout(() => this.checkNow(false), STARTUP_DELAY_MS);
  }

  /** Asks GitHub for a newer version. `manual` checks report errors and ignore skipped versions. */
  async checkNow(manual: boolean) {
    if (this.status.kind === "checking" || this.status.kind === "downloading") return;
    this.status = { kind: "checking" };
    try {
      const found = await check();
      this.lastChecked = Date.now();
      this.savePrefs();
      this.update = found;
      if (!found) {
        this.status = { kind: "current" };
        return;
      }
      this.status = { kind: "available" };
      if (manual || found.version !== this.skipped) this.prompt = true;
    } catch (e) {
      // A failed check at startup (offline, GitHub down) stays quiet; a manual one says why.
      this.status = manual ? { kind: "error", message: describe(e) } : { kind: "idle" };
    }
  }

  setAuto(on: boolean) {
    this.auto = on;
    this.savePrefs();
  }

  /** "Skip this version": no more automatic prompts until a newer one comes out. */
  skip() {
    if (this.update) this.skipped = this.update.version;
    this.savePrefs();
    this.prompt = false;
  }

  later() {
    this.prompt = false;
  }

  /** The release page for the new version (or all releases). */
  releaseUrl() {
    return this.update ? `${RELEASES_URL}/tag/v${this.update.version}` : RELEASES_URL;
  }

  async openReleasePage() {
    try {
      await openUrl(this.releaseUrl());
    } catch (e) {
      this.status = { kind: "error", message: describe(e) };
    }
  }

  /** Downloads, verifies and installs the update, then restarts ProCalc. */
  async install() {
    const u = this.update;
    if (!u) return;
    if (this.portable) {
      await this.openReleasePage();
      this.prompt = false;
      return;
    }
    let done = 0;
    let total: number | null = null;
    this.status = { kind: "downloading", done, total };
    try {
      await u.downloadAndInstall((event) => {
        if (event.event === "Started") total = event.data.contentLength ?? null;
        else if (event.event === "Progress") done += event.data.chunkLength;
        this.status = { kind: "downloading", done, total };
      });
      // On Windows the installer has already closed ProCalc and will reopen it; elsewhere
      // the new files are in place and a restart picks them up.
      this.status = { kind: "restarting" };
      await relaunch();
    } catch (e) {
      this.status = { kind: "error", message: describe(e) };
    }
  }

  private loadPrefs() {
    try {
      const raw = localStorage.getItem(PREFS_KEY);
      if (!raw) return;
      const p = JSON.parse(raw) as Partial<Prefs>;
      if (typeof p.auto === "boolean") this.auto = p.auto;
      if (typeof p.skipped === "string") this.skipped = p.skipped;
      if (typeof p.lastChecked === "number") this.lastChecked = p.lastChecked;
    } catch {
      /* storage unavailable: defaults apply */
    }
  }

  private savePrefs() {
    try {
      const p: Prefs = { auto: this.auto, skipped: this.skipped, lastChecked: this.lastChecked };
      localStorage.setItem(PREFS_KEY, JSON.stringify(p));
    } catch {
      /* storage unavailable: preferences just won't persist */
    }
  }
}

function describe(e: unknown): string {
  const text = String(e instanceof Error ? e.message : e);
  if (/network|connect|dns|timed? ?out|offline|request/i.test(text)) {
    return "Couldn't reach the update server. Check your internet connection and try again.";
  }
  return text;
}

/** The "What's new" part of the release notes, if the release has one. */
export function whatsNew(body: string | undefined): string {
  if (!body) return "";
  const m = body.match(/##\s*What's new\s*\n([\s\S]*?)(?=\n##\s|$)/i);
  return m ? m[1].trim() : "";
}

export function formatBytes(n: number): string {
  if (n < 1024 * 1024) return `${Math.max(1, Math.round(n / 1024))} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

export const updates = new Updates();
