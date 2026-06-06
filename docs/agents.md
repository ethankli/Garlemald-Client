# Working an issue with an AI coding agent

Garlemald Client is built to be contributed to with **AI coding agents** —
[Claude Code](https://claude.com/claude-code) / the Claude Agent SDK, and
[OpenAI Codex](https://openai.com/codex/) / Codex CLI. This page explains how to
point one at an issue and get a green PR out the other end.

The agent's in-repo instructions live in two files at the repo root:

- **[`CLAUDE.md`](../CLAUDE.md)** — read by Claude Code and the Claude Agent SDK.
- **[`AGENTS.md`](../AGENTS.md)** — read by OpenAI Codex and other `AGENTS.md`-aware tools.

Both capture the same essentials — how to build/run, where the modules live, the
AGPL header rule, the `develop` → PR flow, the `fmt`/`clippy`/`build`/`test` gates,
and the cross-platform (Wine/WebView) caveat — so whichever agent you use already
knows the house rules. **Keep the two files in sync** when you change one.

---

## Prerequisites

A working dev environment (see [`dev-environment.md`](dev-environment.md)) plus one
agent toolchain:

### Claude
- **Claude Code (CLI):** install per the
  [Claude Code docs](https://docs.claude.com/en/docs/claude-code), run `claude` in
  your checkout. Authenticate with a Claude.ai (Pro/Max) login or
  `ANTHROPIC_API_KEY`.
- **claude.ai/code (web)** or the **Claude Agent SDK** (needs `ANTHROPIC_API_KEY`).

### OpenAI
- **Codex CLI / Codex:** install per the
  [Codex docs](https://developers.openai.com/codex/), run `codex` in your checkout.
  Authenticate with a ChatGPT login or `OPENAI_API_KEY`.

The agent reads `CLAUDE.md` / `AGENTS.md` automatically on startup — you don't paste
them in.

---

## The loop

1. **Fork & clone**, and add the upstream remote:

   ```sh
   git clone git@github.com:<you>/Garlemald-Client.git
   cd Garlemald-Client
   git remote add upstream https://github.com/swstegall/Garlemald-Client.git
   ```

2. **Branch off `develop`** (never work on `develop`/`main` directly):

   ```sh
   git fetch upstream
   git switch -c fix/<short-slug> upstream/develop
   ```

3. **Hand the agent the issue.** Start the agent in the checkout and give it the
   issue as its task. A good first instruction:

   > Implement issue #NN. Read CLAUDE.md (or AGENTS.md) first. Before you finish, make
   > sure all four gates pass: `cargo fmt --all --check`,
   > `cargo clippy --all-targets -- -D warnings`, `cargo build --all-targets --locked`,
   > and `cargo test --locked`.

4. **Let it build + test**, then review its diff yourself — you own the PR.

5. **Open a PR into upstream `develop`**, linking the issue. CI runs the gates across
   Windows/macOS/Linux; keep them green.

See [`../CONTRIBUTING.md`](../CONTRIBUTING.md) for the full contributor workflow
(access, issue selection, PR etiquette).

---

## The gates the agent must satisfy

A PR cannot merge unless CI is green. These are the exact commands (also in
`.github/workflows/ci.yml`); have the agent run them before finishing:

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo build --all-targets --locked
cargo test --locked
```

`--locked` means a dependency change must come with the updated `Cargo.lock`.

---

## Cross-platform caveat

This is a launcher whose hardest behavior is **OS-specific** (Wine prefix/runtime on
macOS/Linux, native Win32 + in-memory patching on Windows, the WebView, file-system
paths). An agent on one OS can compile and reason about all of it, but it **cannot
fully exercise another OS's launch path**. So:

- Keep platform-specific code behind the existing `platform/` abstraction.
- Lean on CI — it builds/tests on all three OSes.
- Call out in the PR anything that could only be verified on one platform.

---

## Guardrails

Spelled out in `CLAUDE.md` / `AGENTS.md` so the agent already knows them; verify it
honoured them:

- **AGPL license header on every new Rust source file** (incl. `build.rs`), copied
  verbatim from a sibling file. Markdown docs carry none. New upstream code → extend
  `NOTICE.md`.
- **Never cross-commit between repos.** This is one of several independent repos in a
  shared workspace; an agent commits to this one only.
- **Keep CI green** before raising a PR.
- **Branch off `develop`, PR into `develop`** (see [`RELEASING.md`](RELEASING.md)).

---

## Where to read next

- [`../CLAUDE.md`](../CLAUDE.md) / [`../AGENTS.md`](../AGENTS.md) — what the agent reads.
- [`dev-environment.md`](dev-environment.md) — build, run, log, reset state.
- [`architecture.md`](architecture.md) — the launcher pipeline + module map.
- [`../CONTRIBUTING.md`](../CONTRIBUTING.md) — the human contribution workflow.
