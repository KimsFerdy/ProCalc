// Light, dark, or follow the system. The choice sets data-theme on <html>; app.css defines the
// colours for each, and anything drawn in script (the graph) reads `theme.dark`.

export type ThemeChoice = "system" | "light" | "dark";

const KEY = "procalc.theme";

class Theme {
  choice = $state<ThemeChoice>("system");
  private systemDark = $state(true);
  /** Whether the dark colours are showing now. */
  readonly dark = $derived(this.choice === "system" ? this.systemDark : this.choice === "dark");

  constructor() {
    if (typeof window === "undefined") return;
    try {
      const saved = localStorage.getItem(KEY);
      if (saved === "light" || saved === "dark" || saved === "system") this.choice = saved;
    } catch {
      /* storage unavailable: follow the system */
    }
    const mq = matchMedia("(prefers-color-scheme: dark)");
    this.systemDark = mq.matches;
    mq.addEventListener("change", () => (this.systemDark = mq.matches));
    this.apply();
  }

  set(choice: ThemeChoice) {
    this.choice = choice;
    try {
      localStorage.setItem(KEY, choice);
    } catch {
      /* storage unavailable: the choice lasts until the app closes */
    }
    this.apply();
  }

  private apply() {
    const root = document.documentElement;
    if (this.choice === "system") root.removeAttribute("data-theme");
    else root.dataset.theme = this.choice;
  }
}

export const theme = new Theme();
