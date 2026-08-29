# BackHome

A small, backup tool, written in Rust language, that packs your dotfiles and
config directories into a single `.tar.zst` archive. The set of files to back up
is described in a **gitignore-style** config file.

## Build

You need to have rust toolchain with cargo installed

```sh
cargo build --release
```

## Usage

```sh
# check the how to use
./target/release/BackHome --help

# use the default backhome.conf in the current directory
./target/release/BackHome

# explicit config and output path
./target/release/BackHome -e backhome.conf -o my-backup.tar.zst

# override the home directory used to resolve relative entries
./target/release/BackHome -H /home/someone -e backhome.conf

# choose a zstd compression level (0-21, default 3)
./target/release/BackHome -l 10

# include binary files (images, videos, compiled blobs, etc.)
# by default binary files are skipped
./target/release/BackHome -b
```

Default output file is a `backup-YYYY-MM-DD_HH-MM-SS.tar.zst` in the current working directory.

## LLM Usage

This project uses AI to help generate code, and a significant portion of the codebase is AI-generated. 
However, reviews and quality testing are done entirely by a human. 
New features are also planed by human with the help of AI to best suit the current project codebase.

