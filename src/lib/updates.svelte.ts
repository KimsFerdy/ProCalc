// Updates, the way OBS does them: a quiet check at startup, a dialog when there's something new
// (install now, remind me later, skip this version), and a Check now button in Settings.
//
// Desktop installs read the latest GitHub release's latest.json and verify each update against
// the public key in tauri.conf.json before installing it. Phones and the portable zips can't
// replace themselves: they ask GitHub's API for the latest release and offer its download (on
// Android, the APK, which installs over the old version and keeps everything).

import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";

const PREFS_KEY = "procalc.updates";
const RELEASES_URL = "https://github.com/KimsFerdy/ProCalc/releases";
const LATEST_API = "https://api.github.com/repos/KimsFerdy/ProCalc/releases/latest";
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

/** What the dialog shows about the new version, whichever way it was found. */
export interface UpdateInfo {
  version: string;
  date?: string;
  body?: string;
}

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
  /** False on Android and iOS, where updates come from the store or a new download. */
  selfUpdate = $state(true);
  auto = $state(true);
  skipped = $state<string | null>(null);
  lastChecked = $state<number | null>(null);
  status = $state<UpdateStatus>({ kind: "idle" });
  /** The desktop updater's handle on the new version (installers only). */
  update = $state.raw<Update | null>(null);
  /** The new version, for the dialog. */
  info = $state.raw<UpdateInfo | null>(null);
  /** Where to get it when this copy can't install it itself (phones, portable zips). */
  downloadUrl = $state<string | null>(null);
  /** Whether the "update available" dialog is showing. */
  prompt = $state(false);

  /** This copy offers a download rather than installing (phones and portable zips). */
  get downloadOnly() {
    return this.portable || !this.selfUpdate;
  }

  private started = false;

  /** Reads the saved preferences and, if allowed, checks shortly after launch. Call once. */
  async start() {
    if (this.started) return;
    this.started = true;
    this.loadPrefs();
    try {
      this.version = await getVersion();
      this.selfUpdate = await invoke<boolean>("can_self_update");
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
      const found = this.selfUpdate ? await this.checkInstaller() : await this.checkRelease();
      this.lastChecked = Date.now();
      this.savePrefs();
      this.info = found;
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

  /** Desktop: the signed update feed. */
  private async checkInstaller(): Promise<UpdateInfo | null> {
    const found = await check();
    this.update = found;
    if (!found) return null;
    // The portable zips still learn about the version this way; they download it by hand.
    this.downloadUrl = `${RELEASES_URL}/tag/v${found.version}`;
    return { version: found.version, date: found.date, body: found.body };
  }

  /** Phones: the latest release from GitHub's API, and its APK if it has one. */
  private async checkRelease(): Promise<UpdateInfo | null> {
    const res = await fetch(LATEST_API, { headers: { Accept: "application/vnd.github+json" } });
    if (!res.ok) throw new Error(`The update server answered ${res.status}.`);
    const rel = await res.json();
    const version = String(rel.tag_name ?? "").replace(/^v/, "");
    if (!version || !isNewer(version, this.version)) return null;
    const apk = (rel.assets ?? []).find((a: any) => typeof a?.name === "string" && a.name.endsWith(".apk"));
    this.downloadUrl = apk?.browser_download_url ?? rel.html_url ?? `${RELEASES_URL}/tag/v${version}`;
    return { version, date: rel.published_at, body: rel.body };
  }

  setAuto(on: boolean) {
    this.auto = on;
    this.savePrefs();
  }

  /** "Skip this version": no more automatic prompts until a newer one comes out. */
  skip() {
    if (this.info) this.skipped = this.info.version;
    this.savePrefs();
    this.prompt = false;
  }

  later() {
    this.prompt = false;
  }

  /** The release page for the new version (or all releases). */
  releaseUrl() {
    return this.info ? `${RELEASES_URL}/tag/v${this.info.version}` : RELEASES_URL;
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
    if (this.downloadOnly) {
      try {
        await openUrl(this.downloadUrl ?? this.releaseUrl());
      } catch (e) {
        this.status = { kind: "error", message: describe(e) };
        return;
      }
      this.prompt = false;
      return;
    }
    const u = this.update;
    if (!u) return;
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
  if (!m) return "";
  // Shown as plain text: drop Markdown emphasis and code marks, keep bullets as •, and join
  // the lines Markdown wraps (a line that doesn't start a bullet or follow a blank line).
  return m[1]
    .trim()
    .replace(/\n(?!\s*[-*]\s|\s*\n)\s*/g, " ")
    .replace(/\*\*(.+?)\*\*/g, "$1")
    .replace(/`([^`]+)`/g, "$1")
    .replace(/^\s*[-*]\s+/gm, "• ");
}

/** True when version a is newer than b ("0.3.10" > "0.3.9"). */
export function isNewer(a: string, b: string): boolean {
  const pa = a.split(/[.-]/).map((x) => parseInt(x, 10) || 0);
  const pb = b.split(/[.-]/).map((x) => parseInt(x, 10) || 0);
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (d) return d > 0;
  }
  return false;
}

export function formatBytes(n: number): string {
  if (n < 1024 * 1024) return `${Math.max(1, Math.round(n / 1024))} KB`;
  return `${(n / (1024 * 1024)).toFixed(1)} MB`;
}

export const updates = new Updates();
