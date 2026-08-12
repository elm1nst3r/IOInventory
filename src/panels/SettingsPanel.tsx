import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Check,
  Copy,
  DownloadCloud,
  FolderOpen,
  Plug,
  Monitor,
  Moon,
  Palette,
  RefreshCw,
  ShieldAlert,
  ShieldCheck,
  Sun,
} from "lucide-react";
import { useStore } from "../store";
import { api } from "../lib/api";
import { DOMAIN_LABELS, type Domain, type McpInfo } from "../lib/types";
import { ACCENTS, type ThemeMode } from "../lib/appearance";

/** Sections in page order; drives both the side nav and the scroll spy. */
const SECTIONS = [
  { id: "appearance", label: "Appearance" },
  { id: "scanning", label: "What to scan" },
  { id: "roots", label: "Workspace roots" },
  { id: "mcp", label: "MCP server" },
  { id: "updates", label: "Updates" },
] as const;

const THEME_MODES: { id: ThemeMode; label: string; icon: typeof Sun }[] = [
  { id: "system", label: "System", icon: Monitor },
  { id: "light", label: "Light", icon: Sun },
  { id: "dark", label: "Dark", icon: Moon },
];

/** Order scan sources are grouped in, matching the rest of the app. */
const DOMAIN_ORDER: Domain[] = ["package_manager", "runtime", "project", "ai_agent", "container"];

export default function SettingsPanel() {
  // Destructure the whole store rather than passing a selector: a selector that
  // builds an object would return a new reference every render and loop.
  const {
    settings,
    scanSources,
    settingsSaving,
    toggleSource,
    setAllSources,
    setRoots,
    setMcpAllowWrite,
    themeMode,
    setThemeMode,
    accentId,
    setAccent,
    reduceMotion,
    setReduceMotion,
    appVersion,
    updateStatus,
    updateAvailable,
    checkForUpdates,
    installUpdate,
    updateError,
    setAutoUpdateCheck,
  } = useStore();

  const [mcp, setMcp] = useState<McpInfo | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  const [rootsDraft, setRootsDraft] = useState<string | null>(null);
  const [effectiveRoots, setEffectiveRoots] = useState<string[]>([]);
  const [activeSection, setActiveSection] = useState<string>(SECTIONS[0].id);
  const contentRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    api.mcpInfo().then(setMcp).catch(() => {});
    api.getRoots().then(setEffectiveRoots).catch(() => {});
  }, [settings.roots]);

  const grouped = useMemo(() => {
    return DOMAIN_ORDER.map((domain) => ({
      domain,
      sources: scanSources.filter((s) => s.domain === domain),
    })).filter((g) => g.sources.length > 0);
  }, [scanSources]);

  const offCount = settings.disabled_sources.length;
  const allOn = offCount === 0;

  async function copy(text: string, key: string) {
    try {
      await navigator.clipboard.writeText(text);
      setCopied(key);
      setTimeout(() => setCopied((c) => (c === key ? null : c)), 1800);
    } catch {
      /* clipboard unavailable — the text is on screen to copy by hand */
    }
  }

  function saveRoots() {
    if (rootsDraft === null) return;
    const roots = rootsDraft
      .split("\n")
      .map((r) => r.trim())
      .filter(Boolean);
    setRoots(roots);
    setRootsDraft(null);
  }

  const scrollTo = useCallback(
    (id: string) => {
      document.getElementById(id)?.scrollIntoView({
        behavior: reduceMotion ? "auto" : "smooth",
        block: "start",
      });
    },
    [reduceMotion],
  );

  // Scroll spy: the bottom rootMargin means only sections whose heading has
  // reached the upper third of the pane count as "current", so the highlight
  // tracks what you're reading rather than whatever is merely on screen.
  useEffect(() => {
    const root = contentRef.current;
    if (!root) return;
    const visible = new Set<string>();
    const io = new IntersectionObserver(
      (entries) => {
        for (const e of entries) {
          if (e.isIntersecting) visible.add(e.target.id);
          else visible.delete(e.target.id);
        }
        // Furthest-along wins, not first. The band spans the top third, so
        // several tall sections can be in it at once — and the last section
        // can never reach the top of a scrolled-to-bottom pane, which left
        // "Updates" permanently highlighting "MCP server".
        const current = [...SECTIONS].reverse().find((s) => visible.has(s.id));
        if (current) setActiveSection(current.id);
      },
      { root, rootMargin: "0px 0px -68% 0px", threshold: 0 },
    );
    for (const s of SECTIONS) {
      const el = document.getElementById(s.id);
      if (el) io.observe(el);
    }
    return () => io.disconnect();
  }, []);

  return (
    <div className="settings-layout">
      <nav className="settings-nav" aria-label="Settings sections">
        {SECTIONS.map((s) => (
          <button
            key={s.id}
            className={`settings-nav-item ${activeSection === s.id ? "active" : ""}`}
            aria-current={activeSection === s.id ? "true" : undefined}
            onClick={() => scrollTo(s.id)}
          >
            {s.label}
          </button>
        ))}
      </nav>

      <div className="settings" ref={contentRef}>
        <div className="settings-inner">
      <div className="cleanup-intro">
        <h2>Settings</h2>
        <p>
          Choose how the app looks, what gets scanned, how AI agents connect over MCP,
          and how updates are handled. Changes save automatically.
        </p>
      </div>

      {/* -------------------------------------------------------- Appearance */}
      <section className="util-group" id="appearance">
        <div className="util-group-head">
          <h3>
            <Palette size={15} /> Appearance
          </h3>
          <span>Applies instantly, and only on this machine.</span>
        </div>

        <div className="set-field-label">Theme</div>
        <div className="set-segmented" role="group" aria-label="Theme">
          {THEME_MODES.map((m) => {
            const Icon = m.icon;
            return (
              <button
                key={m.id}
                className={themeMode === m.id ? "active" : ""}
                aria-pressed={themeMode === m.id}
                onClick={() => setThemeMode(m.id)}
              >
                <Icon size={14} /> {m.label}
              </button>
            );
          })}
        </div>
        <p className="set-hint set-toggle-note">
          {themeMode === "system"
            ? "Following your system setting, and switching with it. The sun/moon button in the top bar overrides this."
            : `Always ${themeMode}. Choose System to follow your OS again.`}
        </p>

        <div className="set-field-label">Highlight colour</div>
        <div className="set-swatches">
          {ACCENTS.filter((a) => !a.multi).map((a) => (
            <button
              key={a.id}
              className={`set-swatch ${accentId === a.id ? "active" : ""}`}
              style={{ background: a.gradient }}
              title={a.label}
              aria-label={a.label}
              aria-pressed={accentId === a.id}
              onClick={() => setAccent(a.id)}
            >
              {accentId === a.id && <Check size={15} strokeWidth={3} />}
            </button>
          ))}
        </div>

        <div className="set-field-label">Gradients</div>
        <div className="set-swatches">
          {ACCENTS.filter((a) => a.multi).map((a) => (
            <button
              key={a.id}
              className={`set-swatch ${accentId === a.id ? "active" : ""}`}
              style={{ background: a.gradient }}
              title={a.label}
              aria-label={a.label}
              aria-pressed={accentId === a.id}
              onClick={() => setAccent(a.id)}
            >
              {accentId === a.id && <Check size={15} strokeWidth={3} />}
            </button>
          ))}
        </div>
        <p className="set-hint set-toggle-note">
          Currently <strong>{ACCENTS.find((a) => a.id === accentId)?.label}</strong>. Gradients
          are used on buttons and highlights; a matching solid tone is used where a gradient
          can't go, like text and borders.
        </p>

        <label className={`set-toggle plain ${reduceMotion ? "on" : ""}`}>
          <input
            type="checkbox"
            checked={reduceMotion}
            onChange={(e) => setReduceMotion(e.target.checked)}
          />
          <span className="set-source-text">
            <span className="set-source-name">Reduce motion</span>
            <span className="set-source-desc">
              Turns off transitions and hover animations across the app. Loading spinners
              still animate so you can tell work is happening. Defaults to your system
              accessibility setting.
            </span>
          </span>
        </label>
      </section>

      {/* ---------------------------------------------------------- Scanning */}
      <section className="util-group" id="scanning">
        <div className="util-group-head">
          <h3>What to scan</h3>
          <span>
            {allOn
              ? "Everything is included."
              : `${offCount} source${offCount === 1 ? "" : "s"} switched off.`}
          </span>
          <div className="set-head-actions">
            <button className="btn btn-ghost" disabled={allOn || settingsSaving} onClick={() => setAllSources(true)}>
              Select all
            </button>
            <button
              className="btn btn-ghost"
              disabled={offCount === scanSources.length || settingsSaving}
              onClick={() => setAllSources(false)}
            >
              Clear all
            </button>
          </div>
        </div>
        <p className="set-hint">
          Switching a source off skips its collector entirely, so scans get faster — the
          items disappear from the graph on your next scan. Notes and tags you've written
          are kept, and come back if you switch it on again.
        </p>

        {grouped.map((g) => (
          <div key={g.domain} className="set-group">
            <div className="set-group-label">{DOMAIN_LABELS[g.domain]}</div>
            <div className="set-sources">
              {g.sources.map((s) => {
                const on = !settings.disabled_sources.includes(s.id);
                return (
                  <label key={s.id} className={`set-source ${on ? "" : "off"}`}>
                    <input
                      type="checkbox"
                      checked={on}
                      disabled={settingsSaving}
                      onChange={() => toggleSource(s.id)}
                    />
                    <span className="set-source-text">
                      <span className="set-source-name">{s.label}</span>
                      <span className="set-source-desc">{s.description}</span>
                    </span>
                  </label>
                );
              })}
            </div>
          </div>
        ))}
      </section>

      {/* ------------------------------------------------------------- Roots */}
      <section className="util-group" id="roots">
        <div className="util-group-head">
          <h3>
            <FolderOpen size={15} /> Workspace roots
          </h3>
          <span>Where git repositories are searched for.</span>
        </div>
        <p className="set-hint">
          One directory per line. Leave empty to auto-detect the usual places
          (<code>~/Dev</code>, <code>~/Projects</code>, <code>~/Code</code>, …).
        </p>
        <textarea
          className="set-roots"
          rows={Math.max(3, effectiveRoots.length + 1)}
          spellCheck={false}
          value={rootsDraft ?? (settings.roots.length ? settings.roots.join("\n") : effectiveRoots.join("\n"))}
          onChange={(e) => setRootsDraft(e.target.value)}
          placeholder="/Users/you/Dev"
        />
        <div className="set-roots-actions">
          <button className="btn btn-primary" disabled={rootsDraft === null || settingsSaving} onClick={saveRoots}>
            Save roots
          </button>
          {rootsDraft !== null && (
            <button className="btn btn-ghost" onClick={() => setRootsDraft(null)}>
              Cancel
            </button>
          )}
          {settings.roots.length > 0 && rootsDraft === null && (
            <button className="btn btn-ghost" disabled={settingsSaving} onClick={() => setRoots([])}>
              Reset to auto-detected
            </button>
          )}
        </div>
      </section>

      {/* --------------------------------------------------------------- MCP */}
      <section className="util-group" id="mcp">
        <div className="util-group-head">
          <h3>
            <Plug size={15} /> MCP server
          </h3>
          <span>Let AI agents read this inventory.</span>
        </div>
        <p className="set-hint">
          IO Inventory ships an MCP server, so agents like Claude Code can search your
          packages, repos and models, and read notes and tags — without the app running.
        </p>

        <label className={`set-toggle ${settings.mcp_allow_write ? "on" : ""}`}>
          <input
            type="checkbox"
            checked={settings.mcp_allow_write}
            disabled={settingsSaving}
            onChange={(e) => setMcpAllowWrite(e.target.checked)}
          />
          <span className="set-source-text">
            <span className="set-source-name">
              {settings.mcp_allow_write ? (
                <>
                  <ShieldAlert size={14} /> Write actions allowed
                </>
              ) : (
                <>
                  <ShieldCheck size={14} /> Read-only
                </>
              )}
            </span>
            <span className="set-source-desc">
              {settings.mcp_allow_write
                ? "Agents can install, update and uninstall packages and run cleanups on this machine. Your agent client should still ask before each one."
                : "Agents can read your inventory but cannot change anything. They'll still show you the exact command to run yourself."}
            </span>
          </span>
        </label>
        <p className="set-hint set-toggle-note">
          Takes effect immediately — no config change and no client restart needed, though
          your agent may need to re-list its tools before newly enabled ones appear.
        </p>

        {mcp?.available ? (
          <>
            <div className="set-kv">
              <span>Binary</span>
              <code>{mcp.binary_path}</code>
            </div>
            <div className="set-kv">
              <span>Ledger</span>
              <code>{mcp.db_path}</code>
            </div>

            <div className="set-snippet-head">
              Claude Code — run this once
              <button className="link-btn" onClick={() => copy(mcp.cli_command, "cli")}>
                {copied === "cli" ? <Check size={12} /> : <Copy size={12} />}
                {copied === "cli" ? "Copied" : "Copy"}
              </button>
            </div>
            <pre className="set-snippet">{mcp.cli_command}</pre>

            <div className="set-snippet-head">
              Claude Desktop and other clients — merge into your MCP config
              <button className="link-btn" onClick={() => copy(mcp.config_json, "json")}>
                {copied === "json" ? <Check size={12} /> : <Copy size={12} />}
                {copied === "json" ? "Copied" : "Copy"}
              </button>
            </div>
            <pre className="set-snippet">{mcp.config_json}</pre>
          </>
        ) : (
          <div className="set-note">
            The MCP server isn't bundled in this build — it ships with the packaged app.
            When running from source, build it with <code>npm run mcp:build</code> and point
            your client at <code>src-tauri/target/debug/ioinv-mcp</code>.
          </div>
        )}
      </section>

      {/* ----------------------------------------------------------- Updates */}
      <section className="util-group" id="updates">
        <div className="util-group-head">
          <h3>
            <DownloadCloud size={15} /> Updates
          </h3>
          <span>{appVersion ? `You're on v${appVersion}.` : ""}</span>
        </div>

        <label className={`set-toggle plain ${settings.auto_update_check ? "on" : ""}`}>
          <input
            type="checkbox"
            checked={settings.auto_update_check}
            disabled={settingsSaving}
            onChange={(e) => setAutoUpdateCheck(e.target.checked)}
          />
          <span className="set-source-text">
            <span className="set-source-name">Check for updates on launch</span>
            <span className="set-source-desc">
              {settings.auto_update_check
                ? "When the app starts it asks GitHub whether a newer release exists, and shows a banner if so. Nothing is downloaded or installed until you click Download & install."
                : "The app won't contact the network on its own. Use the button below to check whenever you want."}
            </span>
          </span>
        </label>

        {updateAvailable ? (
          <div className="set-note">
            <strong>Version {updateAvailable.version}</strong> is available.
            {updateAvailable.notes && <p className="set-notes-body">{updateAvailable.notes}</p>}
            <button
              className="btn btn-primary"
              disabled={updateStatus === "downloading"}
              onClick={installUpdate}
            >
              {updateStatus === "downloading" ? "Downloading…" : "Download & install"}
            </button>
          </div>
        ) : (
          <div className="set-roots-actions">
            <button
              className="btn"
              disabled={updateStatus === "checking"}
              onClick={() => checkForUpdates(false)}
            >
              <RefreshCw size={14} className={updateStatus === "checking" ? "spin" : ""} />
              {updateStatus === "checking" ? "Checking…" : "Check for updates"}
            </button>
            {updateError && <span className="set-update-msg">{updateError}</span>}
          </div>
        )}
      </section>
        </div>
      </div>
    </div>
  );
}
