# clnr

Find the caches and build output piling up across your machine, see what they cost, and delete them safely.

clnr is a terminal UI (and a scriptable CLI) that knows where developer tools keep their caches: Cargo, Node,
Python, Go, Gradle/Maven, .NET, Terraform, Bazel, Docker, Swift, and Xcode. It walks your project directories for
build output (`target/`, `node_modules/`, `.terraform/`, `obj/`, ...), measures everything in parallel, and lets you
pick what to delete.

Deletes are **permanent**. Everything about clnr is built around never deleting the wrong thing; see
[Safety](#safety).

## Install

From source (Rust 1.100+):

```sh
cargo install --path crates/clnr
```

Or with Bazel: `bazel build -c opt //:clnr`, then copy `bazel-bin/crates/clnr/clnr` onto your `PATH`.

## Use

```sh
clnr                       # the TUI (piped, it prints the list instead)
clnr list                  # everything found, biggest first
clnr list --json           # machine-readable
clnr providers             # what each tool resolves to, and whether it's installed
clnr clean --older-than 30d --min-size 100MB -n   # what would be deleted
clnr clean --older-than 30d --min-size 100MB      # delete it (asks first)
clnr clean -p bazel --orphans                     # Bazel output bases whose workspace is gone
clnr clean -p docker --match postgres             # Docker images matching "postgres"
clnr clean --all -n                               # everything except caution items, as a preview
```

Projects are searched under `~/code` and `/workspace` by default; use `--root` or the config file to change that.
Global caches (`~/.cargo/registry`, `~/.npm`, the Go module cache, ...) are always included.

Every row says what it is, whose it is, how big it is, and when it last changed: `cargo target ~/code/foo`,
`bazel output base /workspace/svc`, `docker image postgres:17`. A note appears only when something should give you
pause (`caution`, `orphan`).

In the TUI: `space` selects, `a` selects everything that isn't `caution`, `d` deletes, `/` filters, `s` sorts,
`v` switches to a per-project view, `enter` shows details, `?` lists all keys. Sizes update live while builds run.
The mouse works too (wheel, click a row, click its first column to select); `--no-mouse` turns that off.

clnr adapts to the terminal it runs in: narrow screens (down to 40x8, e.g. a phone over SSH) drop less important
columns, paths are shortened from the left, ASCII symbols are used when the locale isn't UTF-8 (or with `--ascii`),
and colors follow `--color`, `NO_COLOR`, and `CLICOLOR_FORCE`.

## Safety

Every artifact has a safety class: `regenerable` (rebuilt by the next build), `redownload` (fetched again on next
use), or `caution` (may hold state you care about, like a `.venv`, a stopped container, or Xcode archives). Caution
items are never bulk-selected, `clean` skips them unless asked by name, and the TUI makes you type `yes`.

Before every delete, and again right before it happens, clnr checks that the path:

- still exists, isn't a symlink, and hasn't moved since the scan
- isn't your home directory, a scan root, a protected path, or anything containing them
- is owned by you (clnr refuses to run `clean` as root)
- is inside what the scan covered and still looks like what was found (the same marker file next to it, the same
  identifying files inside)
- isn't in use: a running `cargo build` or Bazel server blocks it

Directories are renamed away atomically before being removed, so tools never see half a `target/`. If clnr is
interrupted mid-delete, the next confirmed delete finishes the job, but only for directories it can prove are the
same ones (by device and inode). Docker objects are removed through Docker, by exact id, without `--force`.

## Configure

```sh
clnr config init     # writes a commented ~/.config/clnr/config.toml
clnr config check    # reports every problem at once
clnr config show     # the settings in effect
```

You can change roots, exclude or protect paths, turn providers or individual rules off, make rules stricter, and
define new tools in TOML without recompiling. See the template written by `clnr config init`.

## Develop

```sh
mise run check       # tests, clippy, and rustfmt
cargo run -p clnr -- list --root .
```

Provider details are in [docs/providers.md](docs/providers.md); contributor notes are in [AGENTS.md](AGENTS.md).

## License

MIT
