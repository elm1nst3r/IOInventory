/** Friendly display names for collector ids (mirrors the Rust graph labels). */
export const COLLECTOR_LABELS: Record<string, string> = {
  homebrew: "Homebrew",
  "homebrew-cask": "Homebrew Casks",
  npm: "npm (global)",
  pnpm: "pnpm (global)",
  pip: "pip",
  pipx: "pipx",
  cargo: "Cargo",
  gem: "RubyGems",
  runtime: "Language Runtimes",
  "rustup-toolchain": "Rust Toolchains",
  "version-manager": "Version Managers",
  git: "Git Repositories",
  "docker-image": "Docker Images",
  "docker-container": "Docker Containers",
  "claude-skill": "Claude Skills",
  "claude-plugin": "Claude Plugins",
  "claude-command": "Claude Commands",
  "claude-agent": "Claude Agents",
  "mcp-server": "MCP Servers",
  "ai-app": "AI Apps & IDEs",
  "ai-cli": "AI CLIs & Agents",
  ollama: "Ollama Models",
  huggingface: "Hugging Face Cache",
  "python-ai-lib": "Python AI Libraries",
  app: "Applications",
};

export function collectorLabel(c: string): string {
  return COLLECTOR_LABELS[c] ?? c;
}
