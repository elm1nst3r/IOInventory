// Appearance preferences: theme mode, accent colour, reduced motion.
//
// These live in localStorage rather than the SQLite settings row on purpose.
// They're per-device display preferences (the MCP server has no use for them),
// and reading them synchronously at module load means the first paint is
// already correct — a round trip to the backend would flash the wrong theme.

export type ThemeMode = "system" | "light" | "dark";
export type Theme = "dark" | "light";

/**
 * An accent preset. Three tones let one definition work in both themes:
 * `color` on dark, the darker `edge` on light (better contrast on white),
 * with `light` as the companion tone for links and hover states.
 */
export interface Accent {
  id: string;
  label: string;
  color: string;
  light: string;
  edge: string;
  /** Background image for primary buttons and swatches. */
  gradient: string;
  /** Multi-hue presets are grouped separately in the UI. */
  multi?: boolean;
  /**
   * Foreground for text sitting *on* the accent (primary buttons). White is
   * unreadable on the pale presets, so those override it.
   */
  onAccent?: string;
  /**
   * Full surface palette override — a preset that repaints the whole app, not
   * just the highlight. Only applied in dark mode; a preset that ships one
   * should also set `forcesDark`.
   */
  surfaces?: Record<string, string>;
  /** Selecting this preset switches the theme to dark. */
  forcesDark?: boolean;
}

const solid = (id: string, label: string, color: string, light: string, edge: string): Accent => ({
  id,
  label,
  color,
  light,
  edge,
  // Matches the app's original primary-button look: lighter top, deeper base.
  gradient: `linear-gradient(180deg, ${light}, ${color})`,
});

const multi = (
  id: string,
  label: string,
  from: string,
  to: string,
  color: string,
  light: string,
  edge: string,
): Accent => ({
  id,
  label,
  color,
  light,
  edge,
  gradient: `linear-gradient(135deg, ${from}, ${to})`,
  multi: true,
});

export const ACCENTS: Accent[] = [
  // Solids. "blueprint" reproduces the app's original blue exactly.
  solid("blueprint", "Blueprint", "#2f9bf5", "#5bb0ff", "#1f7ad4"),
  solid("violet", "Violet", "#8b5cf6", "#a78bfa", "#6d3fd8"),
  solid("emerald", "Emerald", "#10b981", "#34d399", "#0b8f64"),
  solid("cyan", "Cyan", "#06b6d4", "#22d3ee", "#0490a8"),
  { ...solid("amber", "Amber", "#f59e0b", "#fbbf24", "#b97509"), onAccent: "#3b2500" },
  solid("rose", "Rose", "#f43f5e", "#fb7185", "#c81e3c"),
  solid("graphite", "Graphite", "#64748b", "#94a3b8", "#475569"),

  // Repaints the whole app in phosphor green on black. Everything below is a
  // surface override; the CSS extras (mono type, glow, scanlines) key off
  // `data-accent="matrix"` on the root element.
  {
    ...solid("matrix", "Matrix", "#00ff41", "#7dff9b", "#007a24"),
    onAccent: "#00140a",
    forcesDark: true,
    surfaces: {
      "--bg": "#000000",
      "--bg-1": "#040a04",
      "--bg-2": "#071007",
      "--bg-3": "#0b1a0b",
      "--line": "#12401a",
      "--line-2": "#1d6b28",
      "--text": "#c8ffcf",
      "--text-dim": "#4ee46a",
      "--text-faint": "#2a8a3c",
      "--btn-hover": "#0e260f",
      "--btn-hover-border": "#2a8a3c",
      "--ok": "#00ff41",
    },
  },

  // Gradients. `color`/`light`/`edge` are mid-tones of the ramp, so borders,
  // icons and text stay legible where a gradient can't be used.
  multi("aurora", "Aurora", "#6366f1", "#ec4899", "#a855f7", "#c084fc", "#7c3aed"),
  multi("ocean", "Ocean", "#06b6d4", "#3b82f6", "#1d9bde", "#38bdf8", "#0b7fc4"),
  multi("sunset", "Sunset", "#f97316", "#ec4899", "#f2683d", "#fb923c", "#db4f2a"),
  multi("nebula", "Nebula", "#8b5cf6", "#22d3ee", "#5f9bea", "#7dd3fc", "#5b46c8"),
  multi("sherbet", "Sherbet", "#ff8c42", "#ff5e9c", "#fb6f7c", "#ffa08f", "#cf4667"),
  multi("lagoon", "Lagoon", "#14b8a6", "#8b5cf6", "#6d76d8", "#9aa5ec", "#5449ad"),
  { ...multi("meadow", "Meadow", "#22c55e", "#facc15", "#8cbb3a", "#b3d95f", "#5f8a25"), onAccent: "#152600" },
  multi("ember", "Ember", "#e23c3c", "#7c3a12", "#b8442c", "#d96a4f", "#8f3220"),
];

export const DEFAULT_ACCENT = ACCENTS[0];

export function accentById(id: string): Accent {
  return ACCENTS.find((a) => a.id === id) ?? DEFAULT_ACCENT;
}

// ---------------------------------------------------------------- persistence

const KEY_MODE = "al-theme-mode";
const KEY_ACCENT = "al-accent";
const KEY_MOTION = "al-reduce-motion";
/** Pre-appearance-settings key, holding a resolved "dark"/"light". */
const KEY_LEGACY_THEME = "al-theme";

const darkQuery = () => window.matchMedia("(prefers-color-scheme: dark)");

export function systemTheme(): Theme {
  return darkQuery().matches ? "dark" : "light";
}

export function initialThemeMode(): ThemeMode {
  const saved = localStorage.getItem(KEY_MODE);
  if (saved === "system" || saved === "light" || saved === "dark") return saved;
  // Carry over an explicit choice made before this setting existed, so nobody's
  // theme changes under them on upgrade.
  const legacy = localStorage.getItem(KEY_LEGACY_THEME);
  if (legacy === "light" || legacy === "dark") return legacy;
  return "system";
}

export function initialAccentId(): string {
  const saved = localStorage.getItem(KEY_ACCENT);
  return saved && ACCENTS.some((a) => a.id === saved) ? saved : DEFAULT_ACCENT.id;
}

export function initialReduceMotion(): boolean {
  const saved = localStorage.getItem(KEY_MOTION);
  if (saved === "true") return true;
  if (saved === "false") return false;
  // Default to honouring the OS setting rather than overriding it.
  return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
}

export function saveThemeMode(mode: ThemeMode) {
  localStorage.setItem(KEY_MODE, mode);
}
export function saveAccentId(id: string) {
  localStorage.setItem(KEY_ACCENT, id);
}
export function saveReduceMotion(on: boolean) {
  localStorage.setItem(KEY_MOTION, String(on));
}

// ------------------------------------------------------------------- applying

export function resolveTheme(mode: ThemeMode): Theme {
  return mode === "system" ? systemTheme() : mode;
}

/** Surface variables a preset may override; cleared when it doesn't. */
const SURFACE_VARS = [
  "--bg",
  "--bg-1",
  "--bg-2",
  "--bg-3",
  "--line",
  "--line-2",
  "--text",
  "--text-dim",
  "--text-faint",
  "--btn-hover",
  "--btn-hover-border",
  "--ok",
];

/**
 * Push the resolved appearance onto the document root. Inline custom
 * properties beat any stylesheet rule, so this overrides both the `:root` and
 * `:root[data-theme="light"]` defaults. `data-accent` lets a preset add its own
 * CSS beyond what the variables can express.
 */
export function applyAppearance(theme: Theme, accent: Accent, reduceMotion: boolean) {
  const root = document.documentElement;
  root.dataset.theme = theme;
  root.dataset.reduceMotion = String(reduceMotion);
  root.dataset.accent = accent.id;

  const isLight = theme === "light";
  root.style.setProperty("--accent", isLight ? accent.edge : accent.color);
  root.style.setProperty("--accent-2", isLight ? accent.color : accent.light);
  root.style.setProperty("--accent-edge", accent.edge);
  root.style.setProperty("--accent-grad", accent.gradient);
  root.style.setProperty("--on-accent", accent.onAccent ?? "#ffffff");

  // Full-palette presets only make sense on their intended (dark) ground; in
  // light mode they degrade to a plain accent rather than an unreadable mix.
  const surfaces = !isLight ? accent.surfaces : undefined;
  for (const name of SURFACE_VARS) {
    if (surfaces?.[name]) root.style.setProperty(name, surfaces[name]);
    else root.style.removeProperty(name);
  }
}

/** Call the callback whenever the OS light/dark preference flips. */
export function watchSystemTheme(cb: (t: Theme) => void): () => void {
  const q = darkQuery();
  const handler = (e: MediaQueryListEvent) => cb(e.matches ? "dark" : "light");
  q.addEventListener("change", handler);
  return () => q.removeEventListener("change", handler);
}
