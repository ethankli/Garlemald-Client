# Releasing

Garlemald Client uses [Semantic Versioning 2.0.0](https://semver.org/) driven by
git tags. The **git tag `vX.Y.Z` is the source of truth**; `[package].version` in
`Cargo.toml` (which stamps the launcher binary and the embedded Windows resource)
and `Cargo.lock` are kept in lockstep automatically. This mirrors the
[Garlemald-Server](https://github.com/swstegall/Garlemald-Server) release workflow;
the only difference is `[package].version` here vs. `[workspace.package].version`
there.

## Branching model

- **`develop`** is the default branch and the integration branch for day-to-day
  work. Branch features off `develop` and PR back into it. `develop` is protected
  and requires the CI checks (`fmt` / `clippy` / `build-test` on Linux/macOS/Windows)
  plus an open pull request before merging (no approving review is required, so you
  can merge your own PR once CI is green).
- **`main`** is the protected release branch. A release is cut by opening a PR from
  `develop` into `main`; when it merges, the push to `main` triggers `release.yml`
  (version bump + tag), which in turn triggers `release-binaries.yml` (per-platform
  launcher artifacts + the Discord announcement). **Nothing merged into `develop`
  produces a release or a tag** — only the `develop` → `main` merge does.

Flow: feature → `develop` (CI-gated) → release PR `develop` → `main` (CI-gated) →
automatic bump + tag + GitHub Release with launcher artifacts + Discord announcement.

> `main` intentionally does **not** require a pull request, so the release
> automation can push the bump commit straight to it via `RELEASE_PAT`.
>
> The version bump lands on `main` only (the tag is the source of truth), so
> `develop`'s `Cargo.toml` version may lag the latest tag between releases. This
> does **not** cause merge conflicts — only `release.yml` ever edits the version
> line, so each `develop` → `main` merge keeps `main`'s higher version
> automatically. If you'd rather keep `develop`'s version current, periodically
> merge `main` back into `develop` (a standard git-flow back-merge).

## How it works

`.github/workflows/release.yml` runs on every push/merge to `main` (i.e. when a
`develop` → `main` release PR merges) and:

1. reads the highest existing `vX.Y.Z` tag,
2. picks a bump level (see below),
3. rewrites `[package].version` in `Cargo.toml` and runs `cargo update --workspace`
   to sync `Cargo.lock`,
4. commits that back to `main` as `chore(release): vX.Y.Z`,
5. pushes an annotated `vX.Y.Z` tag, and
6. publishes a GitHub Release with auto-generated notes.

After a release, `git describe --tags` on `main` and `[package].version` report the
same `X.Y.Z`, and `cargo build` stamps the launcher (and its Windows resource
version) with it.

## Choosing the bump level

| Bump      | How to trigger                                                                 |
|-----------|--------------------------------------------------------------------------------|
| **Patch** | Default. Any merge to `main` with no release label → `Z` increments.            |
| **Minor** | Add the **`release:minor`** label to the PR before merging → `Y+1`, `Z=0`.      |
| **Major** | Add the **`release:major`** label to the PR before merging → `X+1`, `Y=0`, `Z=0`. |

Guidance: bump **minor** for a new launcher feature, **major** for a breaking
config-file or Wine-prefix layout change. The label is read from the PR associated
with the merge commit; if both are present, `release:major` wins.

### Manual minor/major (alternative)

Because the next version is computed from the **highest tag**, you can also bump
out-of-band by pushing a tag yourself:

```sh
git tag -a v0.2.0 -m v0.2.0 && git push origin v0.2.0
```

The automation then continues patch-incrementing from there (`v0.2.1`, …).

## One-time setup

The workflow pushes the version-bump commit to **`main`, which is branch-protected**
(requires the `fmt`/`clippy`/`build-test` checks from issue #9). The default
`GITHUB_TOKEN` cannot push to a protected branch, so the workflow uses a Personal
Access Token:

1. Create a **fine-grained PAT** owned by a repo **admin**, scoped to **only**
   `Garlemald-Client`, with **Repository permissions → Contents: Read and write**.
2. Save it as the repository secret **`RELEASE_PAT`**.
3. Keep branch protection's **"Do not allow administrators to bypass" OFF**
   (`enforce_admins: false`). The admin-owned PAT relies on that exemption to push
   the bump commit past the required checks. If you turn admin enforcement on, the
   bump push will be rejected.

The `release:minor` / `release:major` labels must exist in the repo (created as
part of issue #8).

> **Security note.** Because `RELEASE_PAT` is admin-owned and `enforce_admins` is
> off, a leaked token can push to `main` bypassing the required checks — a strictly
> larger capability than vanilla Contents: write. Give it a short expiry and rotate
> on a schedule. The release step also needs `crates.io` reachable (it runs
> `cargo update --workspace`); a registry/network blip fails the run, which is safe
> to re-run.

## Loop prevention

A PAT push re-triggers workflows (unlike the default token), so the bump commit's
push to `main` would re-run this workflow. That loop is broken by the job's `if`
guard, which skips commits authored by `github-actions[bot]`.

The bump commit is **deliberately not** marked `[skip ci]`. `[skip ci]` would
suppress not just this workflow but also the **tag-push** event — and a future
per-platform binary-release workflow (mirroring Garlemald-Server's
`release-binaries.yml`) is expected to trigger on that tag push, so a skip marker
would silently prevent those builds.

## macOS code signing (Developer ID + notarization)

`release-binaries.yml` builds an **ad-hoc (unsigned)** `.app` by default — Gatekeeper
warns on first launch. To ship a Gatekeeper-clean, **Developer ID-signed and
notarized** `.app`, add the secrets below; the workflow **auto-activates** signing +
notarization when they are present (it gates on the cert and API-key secrets and
otherwise falls back to ad-hoc — no workflow edit needed to switch on/off).

What the signed path does: `scripts/package-macos.sh --sign` codesigns the universal
binary and bundle with the **Hardened Runtime** (`--options runtime`), a secure
timestamp, and `assets/entitlements.plist` (the `wry`/WKWebView JIT needs
`com.apple.security.cs.allow-jit`); CI then `xcrun notarytool submit --wait`s the
zip, `stapler staple`s the ticket onto the bundle, re-zips, and runs `spctl` as a
final Gatekeeper gate.

Required repository secrets (all from a **paid** Apple Developer Program account;
you must be Account Holder/Admin). Signing requires **both** `MACOS_CERT_P12_BASE64`
and `ASC_API_KEY_P8_BASE64`; if either is missing the build stays ad-hoc:

| Secret | What it is |
|--------|------------|
| `MACOS_CERT_P12_BASE64` | A **Developer ID Application** cert + private key, exported as `.p12`, base64-encoded. |
| `MACOS_CERT_PWD` | The export password set on the `.p12`. **Must be non-empty** — macOS `security import` cannot import a passwordless `.p12` (it fails MAC verification), so set an export password when you export the cert. |
| `ASC_API_KEY_P8_BASE64` | An App Store Connect **team** API key (`.p8`), base64-encoded (downloads once). |
| `ASC_KEY_ID` | The 10-character Key ID. |
| `ASC_ISSUER_ID` | The Issuer ID (UUID) from App Store Connect → Users and Access → Integrations. |

(The signing identity is derived automatically by SHA-1 hash from the imported
cert, so there is no `MACOS_CERT_IDENTITY` secret to keep in sync.)

(The CI keychain password is generated per run inside the workflow, so it is not a secret you provide.)

Keep branch protection's **"Do not allow administrators to bypass" OFF**
(`enforce_admins: false`) — unrelated to signing, but the same precondition the
release bot relies on.

> **Runtime caveat (verify after the first signed release):** the launcher downloads
> the Sikarugir Wine runtime and **spawns it as a subprocess** (it does not load
> those dylibs in-process, so the notarized launcher needs no
> `disable-library-validation`). `ureq` downloads are usually not quarantined, so the
> spawned Wine should run under the notarized launcher — but confirm the game
> launches on a clean Mac. If Gatekeeper blocks the Wine binary, the fix is small:
> strip `com.apple.quarantine` (and/or ad-hoc re-sign) the runtime in
> `src/platform/macos.rs::ensure_runtime_downloaded` after extraction.

## Seeding

The sequence starts from the `v0.1.0` tag on `main`. The first merge after the
release automation lands bumps it to `v0.1.1` (or the labeled minor/major).
