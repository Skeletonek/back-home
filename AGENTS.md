# AGENTS.md

Guidance for AI agents and contributors working on **BackHome**.

## What this project is

A small Rust backup tool that packs dotfiles / config directories into a single
`.tar.zst` archive. The set of files to back up is described in a
**gitignore-style** entries file (default `backhome.conf`).

## Build & test

```sh
cargo build --release
cargo build            # debug, for local runs
```

There is no test harness yet. Verify behavior manually by building and running
against a temp dir:

```sh
mkdir -p /tmp/bhtest && cd /tmp/bhtest
printf '/abs/path/to/file\n' > conf
/path/to/BackHome -e conf -o out.tar.zst
tar -tf out.tar.zst    # inspect archive contents
```

## CLI flags

| Flag | Default | Purpose |
|------|---------|---------|
| `-e, --entries` | `backhome.conf` | Path to gitignore-style entries file |
| `-o, --output`  | `backup-<timestamp>.tar.zst` | Output archive path |
| `-H, --home`    | `$HOME` | Home dir used to resolve relative entries |
| `-l, --level`   | `3` | zstd compression level (0-21) |
| `-f, --follow-symlinks` | off | Follow symlinks when traversing dirs |
| `-b, --include-binary`  | off | Include binary files (otherwise skipped) |

## Entries file semantics (`backhome.conf`)

- One path/glob per line.
- `#` comments and blank lines are ignored.
- `!pattern` is a **negative** (exclude) entry; takes precedence.
- Relative paths resolve against `$HOME`; absolute paths start with `/`.
- Glob support: `*`, `?`, `[...]` (via the `glob` crate).
- Excluding a directory (gitignore-style) excludes its contents.

## Binary file handling (important)

- By default, **binary files are skipped** (NUL byte in first 512 bytes).
- `-b` / `--include-binary` includes them.
- **Exception:** a literal (non-glob) file/symlink entry listed explicitly is
  **always** backed up, even if binary. Only files found via globs or inside
  listed directories are subject to the binary filter.
- Symbolic links are never treated as binary and are always preserved.

## Project structure

```
BackHome/
├── Cargo.toml          # deps: tar, zstd, globset, glob, walkdir, clap, anyhow, chrono
├── backhome.conf       # example entries file (gitignore-style)
├── README.md           # user docs
└── src/
    └── main.rs         # entire program (single file, ~250 lines)
```

### `src/main.rs` layout

- `Cli` (clap `Parser`) — CLI definition.
- `run()` — entry orchestration: parse, read entries, build include/exclude
  globs, collect candidate files, filter, write archive.
- `add_path()` — walk a dir or insert a file/symlink into the file set.
- `is_excluded()` — gitignore-style exclusion (path + ancestors).
- `is_binary()` — NUL-byte heuristic; symlinks exempt.
- `archive_name_for()` — compute archive-relative path (strip `$HOME`/leading `/`).
- `write_archive()` — build the tar and zstd stream.

## Conventions

- Single-file program; keep logic in `main.rs`.
- Use `anyhow` for errors; `eprintln!` for warnings.
- Don't add comments unless necessary; match existing terse style.
- Run `cargo build` before considering a change done.
