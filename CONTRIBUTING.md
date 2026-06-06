# Contributing to Garlemald Client

Thanks for wanting to help build the cross-platform launcher for FINAL FANTASY XIV
1.23b. This page is the **step-by-step for a brand-new contributor**: from zero to a
merged pull request.

New to the codebase? Read these first — you can go zero → building the launcher →
assigned issue → PR using only these docs:

- [`docs/architecture.md`](docs/architecture.md) — the launcher pipeline (detect →
  patch → Wine → WebView login → launch), the module map, and the client↔server flow.
- [`docs/dev-environment.md`](docs/dev-environment.md) — build/run per OS, logging,
  launcher state, and running against a local Garlemald-Server.
- [`docs/agents.md`](docs/agents.md) — optional: drive an AI coding agent through an issue.

---

## 1. Request access

Development is coordinated through the project **Discord** and a shared **GitHub
project board**.

- Join the Discord: <https://discord.gg/CVjwWs6jnX>.
- Ask a maintainer (in Discord) to add you as a **collaborator** on:
  - the **Garlemald-Client** repository, and
  - the **Garlemald project board** —
    <https://github.com/users/swstegall/projects/4>.

You can read the code and open a fork-based PR without any access, but board access
is what lets you claim issues so two people don't pick up the same thing.

---

## 2. Pick an issue

On the [project board](https://github.com/users/swstegall/projects/4):

1. Choose an issue from the **Ready** column.
2. **Assign it to yourself.**
3. Move the card to **In progress**.

If you're unsure where to start, ask in Discord for a good first issue.

---

## 3. Set up your fork

You contribute from a fork and a feature branch off **`develop`** (the default
integration branch — see the branching model in
[`docs/RELEASING.md`](docs/RELEASING.md)).

```sh
# Fork swstegall/Garlemald-Client on GitHub, then:
git clone git@github.com:<you>/Garlemald-Client.git
cd Garlemald-Client
git remote add upstream https://github.com/swstegall/Garlemald-Client.git

# Always branch off the latest upstream develop:
git fetch upstream
git switch -c <type>/<short-slug> upstream/develop   # e.g. fix/webview-redirect
```

Get the launcher building and running once before you change anything — see
[`docs/dev-environment.md`](docs/dev-environment.md) for the per-OS prerequisites
(the GUI/WebView libraries on Linux, Rosetta on Apple Silicon, the 32-bit target on
Windows).

> **Branch off `develop`, never `main`.** `main` is the release branch; `develop` is
> where work integrates.

---

## 4. Make the change

- Match the surrounding code's style and conventions.
- **Every new Rust source file (`.rs`, including `build.rs`) needs the AGPL license
  header.** Copy it verbatim from a sibling file — don't retype it. Markdown / TOML
  files carry none. If you port code from a new upstream, add it to
  [`NOTICE.md`](NOTICE.md).
- Keep OS-specific code behind the existing `platform/` abstraction. You usually
  can't test another OS locally — see the cross-platform note below.
- Add or update tests where it makes sense.

### Keep CI green

Every PR must pass the gates. Run them locally before you push — they're exactly what
CI runs (`.github/workflows/ci.yml`):

```sh
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo build --all-targets --locked
cargo test --locked
```

`--locked` means a dependency change must come with the updated `Cargo.lock`
committed. (`cargo fmt --all` fixes formatting; `--check` only verifies it.)

> **Cross-platform:** CI runs `fmt`/`clippy` on Linux and `build`/`test` on
> **Windows, macOS, and Linux**. If your change touches `platform/` (Wine, Win32, the
> WebView, paths), say so in the PR and rely on CI to cover the OSes you can't run
> locally.

---

## 5. Open a pull request

```sh
git push -u origin <your-branch>
```

Then open a PR on GitHub:

- **Base:** `swstegall/Garlemald-Client` → **`develop`**.
- **Link the issue** (e.g. "Closes #NN") so it auto-closes on merge.
- Describe what changed and how you verified it (and on which OS).
- Make sure CI is green. `main` is protected and release-driven; you only ever PR
  into `develop`.

After you open the PR, move your board card to the appropriate column (e.g. **In
review**). It merges into `develop` and ships in the next `develop` → `main` release.

---

## Tips

- **Smaller PRs review faster.** Large issues can be split across several PRs.
- **Ask early.** A quick question in Discord beats a wrong-direction PR.
- **Logs help reviewers.** For launcher bugs, `RUST_LOG=<module>=debug` output; for
  game/Wine issues, `<data_dir>/logs/wine.log` (see
  [`docs/dev-environment.md`](docs/dev-environment.md)).
- **AI agents are welcome.** See [`docs/agents.md`](docs/agents.md);
  [`CLAUDE.md`](CLAUDE.md) / [`AGENTS.md`](AGENTS.md) tell the agent the house rules.

Welcome aboard. 🚀
