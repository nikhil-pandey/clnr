# AGENTS.md

clnr finds developer caches and build output and deletes them permanently. Correctness of deletes matters more
than anything else here.

## Layout

- `crates/clnr-core`: model, `Provider` trait, scan (walk + measure), clean (guard, removal, journal), config. No
  UI, no CLI parsing.
- `crates/clnr-providers`: one module per tool. Providers describe locations and rules; they never delete paths.
- `crates/clnr-tui`: Elm-style TUI (`state` → `update` → `Effect`s run by `runtime`; `ui` renders).
- `crates/clnr`: the binary (clap args, defaults, output).
- `docs/providers.md`: what each provider finds. Keep it in sync when changing a provider.

## Commands

- `mise run check`: `bazel test //...` (tests, clippy, rustfmt) and `cargo clippy -D warnings`. Run before committing.
- `cargo run -p clnr -- <args>` for quick local runs.
- Dependencies are added with `cargo add`; Bazel picks them up through rules_rs `crate.from_cargo` (`MODULE.bazel.lock`
  updates on the next build). New non-Rust files read at compile time need `compile_data` in `BUILD.bazel`.
- Releases: `bazel build --config=release //packaging:dist` cross-builds every target from any host.

## Rules for providers

- A global location a user can point anywhere must set `verify`. If there is no reliable marker, only support the
  default location.
- Generic project dir names (`bin`, `build`, `target`) need both a marker next to them and `verify` entries
  inside. `Marker::Always` only with a distinctive `verify`.
- No subprocesses for filesystem discovery. Only managed providers (Docker) run their tool.

## Trying destructive changes

Never run `clean` against real data while developing. Use a fresh `mktemp -d` fixture with
`env -i HOME=<fixture>/home PATH=/usr/bin:/bin`, pin providers with `-p` (Docker is reachable even under `env -i`),
point `--root` at the fixture, and dry-run (`-n`) first. Do not scan `~/code` wholesale.
