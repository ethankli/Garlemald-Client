# Releasing

Garlemald Client uses [Semantic Versioning 2.0.0](https://semver.org/) driven by
git tags. The **git tag `vX.Y.Z` is the source of truth**; `[package].version` in
`Cargo.toml` (which stamps the launcher binary and the embedded Windows resource)
and `Cargo.lock` are kept in lockstep automatically. This mirrors the
[Garlemald-Server](https://github.com/swstegall/Garlemald-Server) release workflow;
the only difference is `[package].version` here vs. `[workspace.package].version`
there.

## How it works

`.github/workflows/release.yml` runs on every push/merge to `main` and:

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

## Seeding

The sequence starts from the `v0.1.0` tag on `main`. The first merge after the
release automation lands bumps it to `v0.1.1` (or the labeled minor/major).
