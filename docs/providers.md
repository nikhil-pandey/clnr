# Providers

What each provider finds and how it identifies it. Notation:

- `~` is the home dir.
- `CACHE` is the platform cache dir: `$XDG_CACHE_HOME` or `~/.cache` on Linux, `~/Library/Caches` on macOS.
- `XDG_CACHE` is `$XDG_CACHE_HOME` or `~/.cache` on both, for tools that use it on macOS too.
- `DATA` is `$XDG_DATA_HOME` or `~/.local/share` on Linux, `~/Library/Application Support` on macOS.

## Rules every provider follows

- **Verify anything configurable.** A location a user can point anywhere (an env var holding the path itself)
  must have a `verify` entry that only the real cache contains. If there is no reliable marker, only the default
  location is used. Fixed, distinctive subpaths under a configurable root (`$CARGO_HOME/registry/cache`) are fine.
- **Project dirs need a marker.** A project rule matches a dir name only next to a marker file, and generic names
  (`bin`, `build`, `target`) also need `verify` entries inside. `Marker::Always` is allowed only together with a
  distinctive `verify` (e.g. `CACHEDIR.TAG`).
- **No subprocesses for filesystem discovery.** Paths come from env vars and each tool's documented defaults. Only
  managed providers (Docker) run their tool, because that is the only way to see what they hold.
- **Tools** listed per provider only drive the "installed" status shown in the sidebar and `clnr providers`.

| Provider | Tools | Globals | Project dirs |
| --- | --- | --- | --- |
| `cargo` | cargo, rustup | registry, git, build-cache, rustup downloads/tmp, shared target dir | `target/` |
| `node` | node, npm, pnpm, yarn, bun | npm, yarn, yarn berry, pnpm store, bun | `node_modules/`, `.next/`, `.nuxt/`, `.turbo/`, `.parcel-cache/`, `.svelte-kit/`, `.angular/` |
| `python` | python3, python, pip, uv, poetry | pip, uv, poetry, pre-commit, Hugging Face hub | `.venv/`, `venv/`, `.tox/`, `.nox/`, `.pytest_cache/`, `.mypy_cache/`, `.ruff_cache/` |
| `go` | go | module cache, build cache | — |
| `jvm` | gradle, mvn, java | gradle caches, gradle wrapper dists, maven repository | Gradle `build/`, Maven `target/` |
| `dotnet` | dotnet | NuGet packages, http/v3/plugins caches | `obj/`, `bin/` |
| `terraform` | terraform, tofu, terragrunt | plugin cache (if configured) | `.terraform/`, `.terragrunt-cache/` |
| `swift` | swift, pod | SwiftPM cache, CocoaPods cache (macOS) | SwiftPM `.build/`, `Pods/` |
| `bazel` | bazel, bazelisk | output bases, install bases, repository cache, bazelisk | — (output bases are owned by workspaces) |
| `docker` | docker | images, stopped containers, build cache (managed) | — |
| `xcode` | xcodebuild | DerivedData, archives, device support, caches (macOS) | — |

---

## cargo

| Rule | Path | Safety | Verify / notes |
| --- | --- | --- | --- |
| `registry-cache` | `$CARGO_HOME/registry/cache` (`CARGO_HOME` defaults to `~/.cargo`) | Redownload | |
| `registry-src` | `$CARGO_HOME/registry/src` | Regenerable | Re-extracted from `registry/cache` |
| `registry-index` | `$CARGO_HOME/registry/index` | Redownload | |
| `git-db` | `$CARGO_HOME/git/db` | Redownload | |
| `git-checkouts` | `$CARGO_HOME/git/checkouts` | Regenerable | |
| `build-cache` | `$CARGO_HOME/build-cache` | Regenerable | |
| `rustup-downloads`, `rustup-tmp` | `$RUSTUP_HOME/{downloads,tmp}` (`RUSTUP_HOME` defaults to `~/.rustup`) | Regenerable | Only when `$RUSTUP_HOME/toolchains/` exists |
| `shared-target` | `$CARGO_TARGET_DIR`, `$CARGO_BUILD_TARGET_DIR`, or `build.target-dir` in `$CARGO_HOME/config.toml` | Regenerable | `CACHEDIR.TAG` or `.rustc_info.json` |

**Project:** `target/` next to `Cargo.toml`, verify `CACHEDIR.TAG` or `.rustc_info.json`.

**In use:** a held `.cargo-lock` in `target/*/` or `target/*/*/` blocks the delete.

## node

| Rule | Path | Safety | Verify / notes |
| --- | --- | --- | --- |
| `npm-cacache`, `npm-npx`, `npm-logs` | `{_cacache,_npx,_logs}` under `$npm_config_cache` (default `~/.npm`) | Redownload (logs: Regenerable) | Distinctive subdirectory names |
| `yarn` | `$YARN_CACHE_FOLDER`, else `CACHE/yarn` (Linux) / `CACHE/Yarn` (macOS) | Redownload | `v6`, `v5`, or `v4` |
| `yarn-berry` | `~/.yarn/berry/cache` | Redownload | |
| `pnpm-store` | `DATA/pnpm/store` (Linux) / `~/Library/pnpm/store` (macOS) | Redownload | `v10` or `v3` |
| `bun` | `~/.bun/install/cache` | Redownload | Default only |

**Project** (marker `package.json`): `node_modules/` (Redownload), `.next/` (verify `cache`, `server`, or
`BUILD_ID`), `.angular/` (verify `cache`), `.nuxt/`, `.turbo/`, `.parcel-cache/`, `.svelte-kit/` (Regenerable).

The pnpm store hardlinks into `node_modules`; sizes count each inode once per artifact, so deleting the store alone
frees less than its size suggests.

## python

| Rule | Path | Safety | Verify / notes |
| --- | --- | --- | --- |
| `pip` | `$PIP_CACHE_DIR`, else `CACHE/pip` | Redownload | `http-v2`, `http`, or `wheels` |
| `uv` | `$UV_CACHE_DIR`, else `XDG_CACHE/uv` | Redownload | `CACHEDIR.TAG` |
| `poetry-cache`, `poetry-artifacts` | `CACHE/pypoetry/{cache,artifacts}` | Redownload | Default only; Poetry's `virtualenvs/` is not touched |
| `pre-commit` | `$PRE_COMMIT_HOME`, else `XDG_CACHE/pre-commit` | Regenerable | `db.db` |
| `huggingface-hub` | `XDG_CACHE/huggingface/hub` | **Caution** | Default only; models can be very large to download again |

**Project:**

| Rule | Dir | Marker | Verify | Safety |
| --- | --- | --- | --- | --- |
| `venv`, `venv-plain` | `.venv`, `venv` | `pyproject.toml`, `requirements.txt`, `setup.py`, `setup.cfg`, `Pipfile`, or `uv.lock` | `pyvenv.cfg` | Caution |
| `tox` | `.tox` | `tox.ini`, `pyproject.toml`, or `setup.cfg` | — | Regenerable |
| `nox` | `.nox` | `noxfile.py` | — | Regenerable |
| `pytest-cache`, `mypy-cache`, `ruff-cache` | `.pytest_cache`, `.mypy_cache`, `.ruff_cache` | Always | `CACHEDIR.TAG` | Regenerable |

## go

| Rule | Path | Safety | Verify |
| --- | --- | --- | --- |
| `modcache` | `$GOMODCACHE`, else first `$GOPATH` entry + `/pkg/mod`, else `~/go/pkg/mod` | Redownload | `cache/download` |
| `build-cache` | `$GOCACHE`, else `CACHE/go-build` | Regenerable | `trim.txt` |

The module cache is read-only by design; removal makes directories writable as it goes. Settings written with
`go env -w` are not read.

## jvm

| Rule | Path | Safety | Verify / notes |
| --- | --- | --- | --- |
| `gradle-caches` | `$GRADLE_USER_HOME/caches` (default `~/.gradle`) | Redownload | `modules-2` |
| `gradle-wrapper` | `$GRADLE_USER_HOME/wrapper/dists` | Redownload | Distinctive subpath |
| `maven-repo` | `~/.m2/repository` | Redownload | Default only (`settings.xml` is not read) |

**Project:** Gradle `build/` next to `build.gradle(.kts)` (verify `classes`, `libs`, `tmp`, `intermediates`, or
`generated`); Maven `target/` next to `pom.xml` (verify `classes`, `test-classes`, `maven-status`, or
`maven-archiver`). Both Regenerable. Cargo and Maven share the `target` name; the marker decides.

## dotnet

| Rule | Path | Safety |
| --- | --- | --- |
| `nuget-packages` | `~/.nuget/packages` | Redownload |
| `nuget-http-cache`, `nuget-v3-cache` | `~/.local/share/NuGet/{http-cache,v3-cache}` | Redownload |
| `nuget-plugins-cache` | `~/.local/share/NuGet/plugins-cache` | Regenerable |

Default locations only (NuGet uses `~/.local/share` on macOS too): `NUGET_PACKAGES` and friends can point anywhere
and these caches have no marker to verify.

**Project** (marker: a `*.csproj`, `*.fsproj`, or `*.vbproj` next to it): `obj/` (verify `project.assets.json`) and
`bin/` (verify `Debug` or `Release`). Both Regenerable.

## terraform

| Rule | Path | Safety | Verify |
| --- | --- | --- | --- |
| `plugin-cache` | `$TF_PLUGIN_CACHE_DIR` (no default) | Redownload | `registry.terraform.io` or `registry.opentofu.org` |

**Project:** `.terraform/` next to a `*.tf`, `*.tf.json`, or `*.tofu` file (verify `providers`, `modules`,
`terraform.tfstate`, or `environment`); `.terragrunt-cache/` next to `terragrunt.hcl`. Both Regenerable.
`.terraform/terraform.tfstate` is backend config; real state (`terraform.tfstate`, `terraform.tfstate.d/`) lives
next to `.terraform/` and is never part of the artifact.

## swift

| Rule | Path | Safety |
| --- | --- | --- |
| `swiftpm-cache` | `CACHE/org.swift.swiftpm` | Redownload |
| `cocoapods-cache` | `CACHE/CocoaPods` (macOS) | Redownload |

**Project:** SwiftPM `.build/` next to `Package.swift` (verify `debug`, `release`, `checkouts`, `repositories`, or
`workspace-state.json`; Regenerable); `Pods/` next to `Podfile` (verify `Manifest.lock`; Redownload).

## bazel

Output user root: `~/.cache/bazel/_bazel_$USER` (Linux) or `/private/var/tmp/_bazel_$USER` (macOS). A root set with
`--output_user_root` in a bazelrc is not read.

| Rule | Path | Safety | Verify / notes |
| --- | --- | --- | --- |
| `output-base` | `<root>/<md5>`, one per workspace | Regenerable | `DO_NOT_BUILD_HERE`; its contents name the owning workspace |
| `install-base` | `<root>/install/<md5>`, one per Bazel version (labelled from `build-label.txt`) | Regenerable | `A-server.jar` or `build-label.txt` |
| `repo-cache` | `<root>/cache/repos` | Redownload | |
| `bazelisk` | `$BAZELISK_HOME`, else `CACHE/bazelisk` | Redownload | `downloads` |

- **Owner and orphans.** An output base belongs to the workspace in `DO_NOT_BUILD_HERE`. It is grouped under that
  project in the projects view, and marked orphaned when the workspace is gone (`--orphans`).
- **In use.** An output base whose `server/server.pid.txt` names a live process is blocked, with the
  `bazel shutdown` command to run. Install bases are blocked while any Bazel server is running.
- Output bases contain read-only trees; removal makes them writable as it goes.
- Bazel dates install-base files years ahead. Measurement ignores future mtimes, so ages stay meaningful.

## docker

Discovered through the `docker` CLI, so the current context, Docker Desktop, Colima, and OrbStack all work. These are
managed artifacts: sized by Docker and deleted through it by exact id, without `--force`, so Docker refuses anything
that came into use after the scan.

| Rule | What | Safety | Delete |
| --- | --- | --- | --- |
| `dangling-images` | Untagged images no container uses, as one artifact | Regenerable | `docker image rm <ids>` |
| `image` | Each tagged image no container uses | Redownload | `docker image rm <id>` |
| `container` | Each stopped container (`exited`, `created`, `dead`) | Caution | `docker container rm <id>` |
| `build-cache` | Reclaimable build cache from `docker system df` | Regenerable | `docker builder prune --all --force` |

Sizes come from `docker image inspect` / `docker container inspect --size`. Image sizes include layers shared with
other images, so the total can overstate what deleting frees.

## xcode (macOS only)

| Rule | Path | Safety | Notes |
| --- | --- | --- | --- |
| `derived-data` | `~/Library/Developer/Xcode/DerivedData/<Name>-<hash>`, one per project | Regenerable | Verify `info.plist`; owner from its `WorkspacePath` (orphan when gone) |
| `module-cache` | `DerivedData/ModuleCache.noindex` | Regenerable | |
| `archives` | `~/Library/Developer/Xcode/Archives` | Caution | Needed to symbolicate or re-export shipped builds |
| `device-support` | `~/Library/Developer/Xcode/{iOS,watchOS,tvOS,visionOS} DeviceSupport/<version>` | Redownload | Copied again when a device connects |
| `xcode-cache` | `CACHE/com.apple.dt.Xcode` | Regenerable | |
| `simulator-caches` | `~/Library/Developer/CoreSimulator/Caches` | Regenerable | |

Not verified on a Mac yet. A custom DerivedData location from Xcode's preferences is not read.
