import { useMemo } from "react";
import {
  Bot,
  Database,
  Library,
  Plug,
  Puzzle,
  Share2,
  Sparkles,
  Store,
  Terminal,
  Users,
} from "lucide-react";
import { useStore } from "../store";
import { formatBytes } from "../lib/api";
import type { Item } from "../lib/types";

/** The collectors that describe an agent itself rather than a capability. */
const AGENT_COLLECTORS = ["ai-cli", "ai-app"];

/**
 * Display names for agent slugs. Only a fallback: when the agent is installed
 * its own item names it, and that name wins — this covers a capability
 * configured for an agent that isn't on this machine.
 */
const AGENT_LABELS: Record<string, string> = {
  claude: "Claude Code",
  "claude-desktop": "Claude",
  codex: "Codex",
  gemini: "Gemini",
  cursor: "Cursor",
  windsurf: "Windsurf",
  copilot: "Copilot",
};

/**
 * Capability sections, ordered the way they read: what agents can do, then
 * where those capabilities came from, then the models underneath.
 */
const SECTIONS: { collector: string; label: string; icon: typeof Bot; blurb: string }[] = [
  {
    collector: "claude-skill",
    label: "Skills",
    icon: Sparkles,
    blurb: "Packaged instructions an agent loads on demand.",
  },
  {
    collector: "mcp-server",
    label: "MCP servers",
    icon: Plug,
    blurb: "Tool servers agents connect to. The ones several agents share are marked.",
  },
  {
    collector: "claude-plugin",
    label: "Plugins",
    icon: Puzzle,
    blurb: "Installed bundles of skills, commands and agents.",
  },
  {
    collector: "claude-marketplace",
    label: "Marketplaces",
    icon: Store,
    blurb: "Sources plugins and skills are installed from.",
  },
  {
    collector: "claude-command",
    label: "Slash commands",
    icon: Terminal,
    blurb: "Custom commands available in the agent.",
  },
  {
    collector: "claude-agent",
    label: "Sub-agents",
    icon: Users,
    blurb: "Specialised agents that can be delegated to.",
  },
  {
    collector: "ollama",
    label: "Local models",
    icon: Database,
    blurb: "Models pulled with Ollama and runnable offline.",
  },
  {
    collector: "huggingface",
    label: "Model cache",
    icon: Database,
    blurb: "Models and datasets cached by the Hugging Face libraries.",
  },
  {
    collector: "python-ai-lib",
    label: "Python AI libraries",
    icon: Library,
    blurb: "ML/AI packages installed for Python.",
  },
];

/** Agents a capability belongs to, as recorded by `agents::link` on the scan. */
function agentsOf(item: Item): string[] {
  const list = item.metadata?.agents;
  return Array.isArray(list) ? list.filter((a: unknown) => typeof a === "string") : [];
}

export default function AiPanel() {
  const inventory = useStore((s) => s.inventory);
  const select = useStore((s) => s.select);
  const selectedKey = useStore((s) => s.selectedKey);

  const model = useMemo(() => {
    const items = (inventory?.items ?? []).filter((i) => i.domain === "ai_agent");
    const agents = items.filter((i) => AGENT_COLLECTORS.includes(i.collector));
    const capabilities = items.filter((i) => !AGENT_COLLECTORS.includes(i.collector));

    // An installed agent names itself; the static table only fills the gaps.
    const names = new Map<string, string>();
    for (const a of agents) {
      const slug = a.metadata?.agent;
      if (typeof slug === "string" && slug) names.set(slug, a.name);
    }
    const nameOf = (slug: string) => names.get(slug) ?? AGENT_LABELS[slug] ?? slug;

    // How much each agent has attached to it — the number that makes an agent
    // card worth looking at.
    const counts = new Map<string, number>();
    for (const c of capabilities) {
      for (const slug of agentsOf(c)) counts.set(slug, (counts.get(slug) ?? 0) + 1);
    }

    const sections = SECTIONS.map((s) => ({
      ...s,
      items: capabilities
        .filter((i) => i.collector === s.collector)
        .sort((a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" })),
    })).filter((s) => s.items.length > 0);

    return {
      agents: agents.sort((a, b) => (counts.get(b.metadata?.agent) ?? 0) - (counts.get(a.metadata?.agent) ?? 0)),
      sections,
      shared: capabilities.filter((i) => agentsOf(i).length > 1),
      capabilityCount: capabilities.length,
      nameOf,
      counts,
    };
  }, [inventory]);

  if (!inventory) {
    return <div className="empty-hint">No scan yet — hit “Scan” to map your machine.</div>;
  }
  if (model.agents.length === 0 && model.capabilityCount === 0) {
    return (
      <div className="empty-hint">
        No AI agents or capabilities found. Check that <strong>Claude Code</strong>,{" "}
        <strong>AI tools &amp; apps</strong> and the model sources are switched on in Settings.
      </div>
    );
  }

  const badges = (item: Item) => {
    const slugs = agentsOf(item);
    if (slugs.length === 0) return null;
    return (
      <span className="ai-badges">
        {slugs.map((slug) => (
          <span key={slug} className={`ai-badge ${slugs.length > 1 ? "shared" : ""}`}>
            {model.nameOf(slug)}
          </span>
        ))}
      </span>
    );
  };

  const row = (item: Item) => {
    // Plugin keys are `name@marketplace`; show the readable half, and say
    // where it came from underneath.
    const display = (item.metadata?.plugin as string) ?? item.name;
    const from = item.metadata?.marketplace ?? item.metadata?.source;
    return (
      <button
        key={item.item_key}
        className={`ai-row ${selectedKey === item.item_key ? "sel" : ""}`}
        onClick={() => select(item.item_key)}
      >
        <span className="ai-row-main">
          <span className="ai-row-name">{display}</span>
          {item.version && <span className="chip-ver">{item.version}</span>}
          {item.size_bytes ? <span className="chip-size">{formatBytes(item.size_bytes)}</span> : null}
        </span>
        <span className="ai-row-sub">
          {typeof from === "string" && from && <span className="ai-from">{from}</span>}
          {badges(item)}
        </span>
      </button>
    );
  };

  return (
    <div className="cleanup ai-panel">
      <div className="cleanup-intro">
        <h2>AI &amp; Agents</h2>
        <p>
          Every agent on this machine and what's layered on top of it — skills, MCP servers,
          plugins and the marketplaces they came from. Capabilities configured for more than one
          agent are listed once and marked as <strong>shared</strong>.
        </p>
      </div>

      {model.agents.length > 0 && (
        <section className="util-group">
          <div className="util-group-head">
            <h3>
              <Bot size={15} /> Agents
            </h3>
            <span>
              {model.agents.length} installed · {model.capabilityCount} capabilities between them
            </span>
          </div>
          <div className="ai-agents">
            {model.agents.map((a) => {
              const slug = a.metadata?.agent as string | undefined;
              const attached = slug ? (model.counts.get(slug) ?? 0) : 0;
              return (
                <button
                  key={a.item_key}
                  className={`ai-agent-card ${selectedKey === a.item_key ? "sel" : ""}`}
                  onClick={() => select(a.item_key)}
                >
                  <span className="ai-agent-head">
                    <strong>{a.name}</strong>
                    {a.version && <span className="chip-ver">{a.version}</span>}
                  </span>
                  {a.metadata?.description && (
                    <span className="ai-agent-desc">{a.metadata.description}</span>
                  )}
                  <span className="ai-agent-foot">
                    {attached > 0 ? `${attached} attached` : "nothing attached"}
                    {a.collector === "ai-cli" ? " · CLI" : " · app"}
                  </span>
                </button>
              );
            })}
          </div>
        </section>
      )}

      {model.shared.length > 0 && (
        <section className="util-group">
          <div className="util-group-head">
            <h3>
              <Share2 size={15} /> Shared across agents
            </h3>
            <span>Configured for more than one agent — one row, every agent that uses it.</span>
          </div>
          <div className="ai-rows">{model.shared.map(row)}</div>
        </section>
      )}

      {model.sections.map((s) => {
        const Icon = s.icon;
        return (
          <section key={s.collector} className="util-group">
            <div className="util-group-head">
              <h3>
                <Icon size={15} /> {s.label} <span className="count-pill">{s.items.length}</span>
              </h3>
              <span>{s.blurb}</span>
            </div>
            <div className="ai-rows">{s.items.map(row)}</div>
          </section>
        );
      })}
    </div>
  );
}
