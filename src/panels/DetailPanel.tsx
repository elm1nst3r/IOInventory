import { useEffect, useState } from "react";
import { revealItemInDir, openUrl, openPath } from "@tauri-apps/plugin-opener";
import { ExternalLink, FolderOpen, FileText } from "lucide-react";
import { useStore } from "../store";
import { formatBytes } from "../lib/api";
import type { Item } from "../lib/types";

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
            <li key={i.item_key} onClick={() => useStore.getState().select(i.item_key)}>
              <span className="dl-name">{i.name}</span>
              {i.version && <span className="dl-ver">{i.version}</span>}
              {i.size_bytes ? <span className="dl-size">{formatBytes(i.size_bytes)}</span> : null}
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
  const info = useStore((s) => s.enrichCache[item.item_key]);
  const enriching = useStore((s) => s.enriching === item.item_key);

  useEffect(() => {
    setWhy(item.why ?? "");
    setSaved(false);
    enrich(item);
  }, [item.item_key]);

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
  const scanOutdated = Boolean(item.metadata?.outdated);
  const outdated = scanOutdated || info?.outdated === true;
  const latest = scanLatest ?? info?.latest_version ?? undefined;
  const current = item.version ?? info?.installed_version ?? undefined;
  const canShowUpToDate =
    latest != null || info?.outdated === false || (info != null && info.latest_version != null);

  async function save() {
    await onSave(item.item_key, item.note ?? "", why);
    setSaved(true);
    setTimeout(() => setSaved(false), 1500);
  }

  return (
    <aside className="detail">
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

      <div className="note-editor">
        <label>Why is this here? (note)</label>
        <textarea
          value={why}
          placeholder="e.g. used by the Faceswap project for GPU inference"
          onChange={(e) => setWhy(e.target.value)}
        />
        <button className="btn btn-primary" onClick={save}>
          {saved ? "Saved ✓" : "Save note"}
        </button>
      </div>
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
