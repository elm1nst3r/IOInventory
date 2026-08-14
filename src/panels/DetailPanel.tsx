import { useEffect, useMemo, useState } from "react";
import { revealItemInDir, openUrl, openPath } from "@tauri-apps/plugin-opener";
import {
  ExternalLink,
  FolderOpen,
  FileText,
  ArrowUpCircle,
  ArrowLeft,
  PackagePlus,
  Trash2,
  AlertTriangle,
  X,
} from "lucide-react";
import { useStore } from "../store";
import RunningLabel from "./RunningLabel";
import { api, formatBytes } from "../lib/api";
import type { ActionInfo, ActionResult, Item } from "../lib/types";

type Action = "update" | "delete" | "install";

const ACTION_LABELS: Record<Action, { question: string; confirm: string }> = {
  update: { question: "Update this item?", confirm: "Confirm update" },
  delete: { question: "Uninstall this item?", confirm: "Confirm uninstall" },
  install: { question: "Install this item?", confirm: "Confirm install" },
};

/**
 * "← Applications" above the detail, when you drilled in from somewhere.
 * Naming the destination matters: a bare arrow doesn't say whether you're
 * going back to the collector you came from or to some earlier item.
 */
function BackLink() {
  const previous = useStore((s) => s.selectionHistory[s.selectionHistory.length - 1]);
  const selectBack = useStore((s) => s.selectBack);
  const inventory = useStore((s) => s.inventory);
  const graph = useStore((s) => s.graph);
  if (!previous) return null;
  const label =
    inventory?.items.find((i) => i.item_key === previous)?.name ??
    graph?.nodes.find((n) => n.id === previous)?.label ??
    "Back";
  return (
    <button className="detail-back" onClick={selectBack} title={`Back to ${label}`}>
      <ArrowLeft size={13} />
      <span>{label}</span>
    </button>
  );
}

export default function DetailPanel() {
  const selectedKey = useStore((s) => s.selectedKey);
  const inventory = useStore((s) => s.inventory);
  const graph = useStore((s) => s.graph);
  const saveNote = useStore((s) => s.saveNote);

  if (!selectedKey || !inventory) {
    return (
      <aside className="detail">
        <div className="detail-empty">
          Select a node to see details, sizes, and add a note.
        </div>
      </aside>
    );
  }

  const gnode = graph?.nodes.find((n) => n.id === selectedKey);
  const item = inventory.items.find((i) => i.item_key === selectedKey);

  if (item) {
    return <ItemDetail key={item.item_key} item={item} onSave={saveNote} />;
  }

  // Aggregate node (root / domain / collector): list what's under it.
  if (gnode) {
    const childItems = inventory.items.filter((i) => {
      if (gnode.kind === "collector") {
        return `c:${i.domain}:${i.collector}` === gnode.id;
      }
      if (gnode.kind === "domain") return `d:${i.domain}` === gnode.id;
      return true;
    });
    return (
      <aside className="detail">
        <BackLink />
        <div className="detail-head">
          <div className="detail-kind">{gnode.kind}</div>
          <h2>{gnode.label}</h2>
          <div className="detail-meta">
            {gnode.count} items
            {gnode.size_bytes ? ` · ${formatBytes(gnode.size_bytes)}` : ""}
          </div>
        </div>
        <ul className="detail-list">
          {childItems.slice(0, 400).map((i) => (
            <li key={i.item_key}>
              <button type="button" onClick={() => useStore.getState().select(i.item_key)}>
                <span className="dl-name">{i.name}</span>
                {i.version && <span className="dl-ver">{i.version}</span>}
                {i.size_bytes ? <span className="dl-size">{formatBytes(i.size_bytes)}</span> : null}
              </button>
            </li>
          ))}
        </ul>
      </aside>
    );
  }

  return <aside className="detail" />;
}

function ItemDetail({
  item,
  onSave,
}: {
  item: Item;
  onSave: (k: string, note: string, why: string) => Promise<void>;
}) {
  const [why, setWhy] = useState(item.why ?? "");
  const [saved, setSaved] = useState(false);
  const enrich = useStore((s) => s.enrich);
  const scan = useStore((s) => s.scan);
  const info = useStore((s) => s.enrichCache[item.item_key]);
  const enriching = useStore((s) => s.enriching === item.item_key);

  const [actions, setActions] = useState<ActionInfo | null>(null);
  const [confirm, setConfirm] = useState<Action | null>(null);
  const [running, setRunning] = useState<Action | null>(null);
  const [runStarted, setRunStarted] = useState<number | null>(null);
  const [actionResult, setActionResult] = useState<ActionResult | null>(null);

  const setItemTags = useStore((s) => s.setItemTags);
  const setView = useStore((s) => s.setView);
  const readOnly = useStore((s) => s.viewingSnapshot !== null);
  const liveInventory = useStore((s) => s.liveInventory);
  // While a snapshot is on screen its items are historical: some may no longer
  // be on the machine. Those are the ones worth offering to install — anything
  // in the live inventory is by definition already here.
  const missingLocally =
    readOnly &&
    liveInventory !== null &&
    !liveInventory.items.some((i) => i.item_key === item.item_key);
  // NOTE: derive with useMemo — a selector that returns a fresh array each call
  // sends useSyncExternalStore into an infinite render loop.
  const inventoryItems = useStore((s) => s.inventory?.items);
  const allTagSuggestions = useMemo(
    () => [...new Set((inventoryItems ?? []).flatMap((i) => i.tags ?? []))].sort(),
    [inventoryItems],
  );
  const [tagInput, setTagInput] = useState("");
  const [editError, setEditError] = useState<string | null>(null);
  const tags = item.tags ?? [];

  async function addTag(raw: string) {
    const t = raw.trim().replace(/^#/, "");
    if (!t || tags.includes(t)) {
      setTagInput("");
      return;
    }
    try {
      await setItemTags(item.item_key, [...tags, t]);
      setTagInput("");
      setEditError(null);
    } catch (error) {
      setEditError(String(error));
    }
  }
  async function removeTag(t: string) {
    try {
      await setItemTags(
        item.item_key,
        tags.filter((x) => x !== t),
      );
      setEditError(null);
    } catch (error) {
      setEditError(String(error));
    }
  }

  useEffect(() => {
    setWhy(item.why ?? "");
    setSaved(false);
    enrich(item);
    setActions(null);
    setConfirm(null);
    setActionResult(null);
    setEditError(null);
    api.itemActions(item).then(setActions).catch(() => setActions(null));
  }, [item.item_key]);

  async function doAction(action: Action) {
    setRunning(action);
    setRunStarted(Date.now());
    try {
      const r = await api.runItemAction(item, action);
      setActionResult(r);
      setConfirm(null);
      // Re-scan so lists/graph reflect the change; give a beat to read the result.
      if (r.success) setTimeout(() => scan(), 1400);
    } catch (error) {
      setConfirm(null);
      setActionResult({
        command: "",
        output: String(error),
        success: false,
      });
    } finally {
      setRunning(null);
      setRunStarted(null);
    }
  }

  const stacks: string[] = item.metadata?.stacks ?? [];
  const launch: string | undefined = item.metadata?.launch_cmd;
  const remote: string | undefined = item.metadata?.remote;
  const lastCommit: string | undefined = item.metadata?.last_commit;
  const remoteUrl = remote ? gitRemoteToUrl(remote) : null;
  const remoteHost = remoteUrl ? gitHostLabel(remoteUrl) : "Open remote";
  // Description / homepage can come from live enrichment or baked-in metadata.
  const description = info?.description ?? (item.metadata?.description as string | undefined);
  const homepage = info?.homepage ?? (item.metadata?.homepage as string | undefined);
  // A file worth opening directly (a skill's SKILL.md, etc.).
  const openFile = item.metadata?.file as string | undefined;

  // Update status: prefer scan-time signal, fall back to enrichment.
  const scanLatest: string | undefined = item.metadata?.latest;
  // Which management actions this item gets. In the live view that's update /
  // uninstall; in a snapshot the only sensible one is putting back something
  // that's no longer here.
  const isApp = item.collector === "app";
  const offered = !actions
    ? []
    : readOnly
      ? missingLocally && actions.install
        ? [{ action: "install" as const, label: "Install", Icon: PackagePlus }]
        : []
      : [
          actions.update && { action: "update" as const, label: "Update", Icon: ArrowUpCircle },
          actions.delete && {
            action: "delete" as const,
            label: isApp && !item.metadata?.cask ? "Move to Trash" : "Uninstall",
            Icon: Trash2,
          },
        ].filter((b) => !!b);

  const scanOutdated = Boolean(item.metadata?.outdated);
  const outdated = scanOutdated || info?.outdated === true;
  const latest = scanLatest ?? info?.latest_version ?? undefined;
  const current = item.version ?? info?.installed_version ?? undefined;
  const canShowUpToDate =
    latest != null || info?.outdated === false || (info != null && info.latest_version != null);

  async function save() {
    try {
      await onSave(item.item_key, item.note ?? "", why);
      setEditError(null);
      setSaved(true);
      setTimeout(() => setSaved(false), 1500);
    } catch (error) {
      setEditError(String(error));
    }
  }

  return (
    <aside className="detail">
      <BackLink />
      <div className="detail-head">
        <div className="detail-kind">{item.collector}</div>
        <h2>{item.name}</h2>
        {current && <div className="detail-meta">v{current}</div>}
      </div>

      {(description || enriching) && (
        <p className="detail-desc">
          {description ?? <span className="dim">Loading description…</span>}
        </p>
      )}

      {(remoteUrl || homepage || openFile || item.source_path) && (
        <div className="quick-actions">
          {remoteUrl && (
            <button className="btn qa-btn" onClick={() => openUrl(remoteUrl)}>
              <ExternalLink size={14} /> {remoteHost}
            </button>
          )}
          {homepage && (
            <button className="btn qa-btn" onClick={() => openUrl(homepage)}>
              <ExternalLink size={14} /> Website
            </button>
          )}
          {openFile && (
            <button className="btn qa-btn" onClick={() => openPath(openFile)}>
              <FileText size={14} /> Open {openFile.split("/").pop()}
            </button>
          )}
          {item.source_path && (
            <button className="btn qa-btn" onClick={() => openPath(item.source_path!)}>
              <FolderOpen size={14} /> Open folder
            </button>
          )}
        </div>
      )}

      {canShowUpToDate && (
        <div className={`status-pill ${outdated ? "warn" : "ok"}`}>
          {outdated
            ? `Update available${current && latest ? ` · ${current} → ${latest}` : latest ? ` · ${latest}` : ""}`
            : "Up to date"}
        </div>
      )}

      {!readOnly && actions && offered.length === 0 && actions.note && (
        <div className="manage">
          <div className="manage-note">{actions.note}</div>
        </div>
      )}

      {actions && offered.length > 0 && (
        <div className="manage">
          {missingLocally && (
            <div className="manage-note">
              Not installed on this machine — it was here when the snapshot was taken.
            </div>
          )}
          <div className="manage-btns">
            {offered.map(({ action, label, Icon }) => (
              <button
                key={action}
                className={`btn qa-btn ${action === "delete" ? "manage-del" : ""}`}
                disabled={!actions.available || running !== null}
                onClick={() => {
                  setConfirm(action);
                  setActionResult(null);
                }}
              >
                <Icon size={14} /> {label}
              </button>
            ))}
          </div>
          {!actions.available && (
            <div className="manage-note">Its package manager isn’t on your PATH.</div>
          )}
          {actions.note && <div className="manage-note">{actions.note}</div>}

          {confirm && (
            <div className={`manage-confirm ${confirm === "delete" ? "danger" : ""}`}>
              {confirm === "delete" && <AlertTriangle size={15} className="mc-warn" />}
              <div className="mc-body">
                <div className="mc-q">
                  {confirm === "delete" && isApp && !item.metadata?.cask
                    ? "Move this app to the Trash?"
                    : ACTION_LABELS[confirm].question}
                </div>
                <code className="mc-cmd">{actions[confirm]}</code>
                <div className="mc-actions">
                  <button
                    className={`btn ${confirm === "delete" ? "btn-danger" : "btn-primary"}`}
                    disabled={running !== null}
                    onClick={() => doAction(confirm)}
                  >
                    {running
                      ? <RunningLabel since={runStarted ?? undefined} />
                      : confirm === "delete" && isApp && !item.metadata?.cask
                        ? "Move to Trash"
                        : ACTION_LABELS[confirm].confirm}
                  </button>
                  <button className="btn btn-ghost" onClick={() => setConfirm(null)}>
                    Cancel
                  </button>
                </div>
              </div>
            </div>
          )}

          {actionResult && (
            <div className={`manage-result ${actionResult.success ? "ok" : "fail"}`}>
              <div className="cc-preview-head">
                {actionResult.success ? "Done ✓ — refreshing…" : "Failed"}
              </div>
              <pre>{actionResult.output}</pre>
            </div>
          )}
        </div>
      )}

      <dl className="detail-fields">
        {info?.installed_at && <Field label="Installed / updated" value={info.installed_at} />}
        {item.size_bytes != null && (
          <Field label="Size" value={formatBytes(item.size_bytes)} />
        )}
        {stacks.length > 0 && <Field label="Stack" value={stacks.join(", ")} />}
        {launch && <Field label="Launch" value={launch} mono />}
        {lastCommit && <Field label="Last commit" value={lastCommit} />}
        {item.source_path && (
          <div className="field">
            <dt>Path</dt>
            <dd>
              <span className="mono">{item.source_path}</span>
              <button
                className="link-btn"
                onClick={() => revealItemInDir(item.source_path!)}
              >
                Reveal
              </button>
            </dd>
          </div>
        )}
      </dl>

      <div className="tags-editor">
        <div className="field-label">Tags · views</div>
        <div className="tag-chips">
          {tags.map((t) => (
            <span key={t} className="tag-chip">
              <button
                className="tag-view"
                title={`Show the #${t} view`}
                onClick={() => setView(t)}
              >
                #{t}
              </button>
              {!readOnly && (
                <button className="tag-x" title="Remove tag" aria-label={`Remove tag ${t}`} onClick={() => void removeTag(t)}>
                  <X size={11} />
                </button>
              )}
            </span>
          ))}
          {tags.length === 0 && <span className="tag-empty">No tags</span>}
        </div>
        {!readOnly && (
          <>
            <input
              className="tag-input"
              aria-label="Add a tag"
              list="tag-suggestions"
              value={tagInput}
              placeholder="Add a tag (e.g. favorite) and press Enter"
              onChange={(e) => setTagInput(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === ",") {
                  e.preventDefault();
                  void addTag(tagInput);
                }
              }}
            />
            <datalist id="tag-suggestions">
              {allTagSuggestions.map((t) => (
                <option key={t} value={t} />
              ))}
            </datalist>
          </>
        )}
      </div>

      {editError && <div className="edit-error" role="alert">{editError}</div>}

      {readOnly ? (
        why ? (
          <div className="note-editor">
            <div className="field-label">Why is this here? (note)</div>
            <p className="detail-desc">{why}</p>
          </div>
        ) : null
      ) : (
        <div className="note-editor">
          <label htmlFor={`item-note-${item.item_key}`}>Why is this here? (note)</label>
          <textarea
            id={`item-note-${item.item_key}`}
            value={why}
            placeholder="e.g. used by the Faceswap project for GPU inference"
            onChange={(e) => setWhy(e.target.value)}
          />
          <button className="btn btn-primary" onClick={save}>
            {saved ? "Saved ✓" : "Save note"}
          </button>
        </div>
      )}
    </aside>
  );
}

/** Normalize a git remote (ssh or https) into a browsable https URL. */
function gitRemoteToUrl(remote: string): string | null {
  let r = remote.trim();
  if (!r) return null;
  r = r.replace(/\.git$/, "");
  // scp-like: git@github.com:user/repo
  const scp = r.match(/^[a-zA-Z0-9_.-]+@([^:]+):(.+)$/);
  if (scp) return `https://${scp[1]}/${scp[2]}`;
  // ssh://git@host/user/repo
  if (r.startsWith("ssh://")) {
    const m = r.match(/^ssh:\/\/(?:[^@]+@)?([^/]+)\/(.+)$/);
    if (m) return `https://${m[1]}/${m[2]}`;
  }
  if (r.startsWith("http://")) return "https://" + r.slice(7);
  if (r.startsWith("https://")) return r;
  return null;
}

/** A friendly button label based on the host. */
function gitHostLabel(url: string): string {
  const host = shortUrl(url);
  if (host.includes("github")) return "Open on GitHub";
  if (host.includes("gitlab")) return "Open on GitLab";
  if (host.includes("bitbucket")) return "Open on Bitbucket";
  return "Open remote";
}

function shortUrl(url: string): string {
  try {
    return new URL(url).host.replace(/^www\./, "");
  } catch {
    return url;
  }
}

function Field({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="field">
      <dt>{label}</dt>
      <dd className={mono ? "mono" : ""}>{value}</dd>
    </div>
  );
}
