use anyhow::{anyhow, Context, Result};
use clap::Parser;
use globset::{Glob, GlobSetBuilder};
use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::exit;
use walkdir::WalkDir;

#[derive(Parser)]
#[command(
    name = "BackHome",
    about = "Back up dotfiles/configs into a .tar.zst archive using a gitignore-style entries file"
)]
struct Cli {
    /// Path to the gitignore-style entries file
    #[arg(short, long, default_value = "backhome.conf")]
    entries: PathBuf,

    /// Output archive path (.tar.zst). Defaults to backup-<timestamp>.tar.zst in cwd.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Override the home directory used to resolve relative entries
    #[arg(short = 'H', long)]
    home: Option<PathBuf>,

    /// zstd compression level (0-21, default 3)
    #[arg(short = 'l', long, default_value_t = 3)]
    level: i32,

    /// Follow symbolic links when traversing directories (default: do not follow)
    #[arg(short = 'f', long)]
    follow_symlinks: bool,

    /// Include binary files (detected via NUL byte). Default: skip them.
    #[arg(short = 'b', long)]
    include_binary: bool,

    /// Suppress all non-error output (progress, warnings, summary)
    #[arg(short = 'q', long, conflicts_with = "verbose")]
    quiet: bool,

    /// Show per-file paths while backing up (keeps progress bar)
    #[arg(short = 'v', long, conflicts_with = "quiet")]
    verbose: bool,
}

const GLOB_META: &[char] = &['*', '?', '[', ']'];

fn has_glob_meta(s: &str) -> bool {
    s.chars().any(|c| GLOB_META.contains(&c))
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {:#}", e);
        exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    let home = cli
        .home
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .ok_or_else(|| anyhow!("could not determine HOME; pass --home"))?;
    let home = home.canonicalize().unwrap_or(home);

    let entries_text = std::fs::read_to_string(&cli.entries)
        .with_context(|| format!("reading entries file {}", cli.entries.display()))?;

    let mut include_patterns: Vec<String> = Vec::new();
    let mut exclude_patterns: Vec<String> = Vec::new();

    for raw in entries_text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(rest) = line.strip_prefix('!') {
            let p = rest.trim();
            if !p.is_empty() {
                exclude_patterns.push(p.to_string());
            }
        } else {
            include_patterns.push(line.to_string());
        }
    }

    // Resolve exclude globs to absolute patterns (relative ones prefixed with $HOME).
    let mut ex_builder = GlobSetBuilder::new();
    for p in &exclude_patterns {
        let abs = if p.starts_with('/') {
            p.clone()
        } else {
            home.join(p).to_string_lossy().into_owned()
        };
        let g = Glob::new(&abs).with_context(|| format!("invalid exclude glob: {abs}"))?;
        ex_builder.add(g);
    }
    let exclude_set = ex_builder.build()?;

    // Collect candidate files from include entries.
    let mut files: HashSet<PathBuf> = HashSet::new();
    // Literal (non-glob) file/symlink entries are always backed up, even if
    // binary, since the user spelled out exactly what they want.
    let mut explicit_files: HashSet<PathBuf> = HashSet::new();
    for inc in &include_patterns {
        let abs = if inc.starts_with('/') {
            PathBuf::from(inc)
        } else {
            home.join(inc)
        };

        if has_glob_meta(inc) {
            let pattern = abs.to_string_lossy().into_owned();
            let glob = glob::glob(&pattern)
                .with_context(|| format!("invalid include glob: {pattern}"))?;
            for entry in glob {
                match entry {
                    Ok(p) => add_path(&p, &mut files, cli.follow_symlinks)?,
                    Err(e) => {
                        if !cli.quiet {
                            eprintln!("warn: glob error: {e}");
                        }
                    }
                }
            }
        } else if !abs.exists() {
            if !cli.quiet {
                eprintln!("warn: include path does not exist, skipping: {}", abs.display());
            }
        } else {
            add_path(&abs, &mut files, cli.follow_symlinks)?;
            let mt = std::fs::symlink_metadata(&abs)?;
            if mt.file_type().is_file() || mt.file_type().is_symlink() {
                explicit_files.insert(abs.clone());
            }
        }
    }

    // Apply negative (exclude) entries. A path is excluded if it matches an
    // exclude pattern directly, or if any of its ancestor directories match
    // (gitignore semantics: excluding a directory excludes its contents too).
    let mut included: Vec<PathBuf> = files
        .into_iter()
        .filter(|f| !is_excluded(f, &exclude_set))
        .filter(|f| cli.include_binary || explicit_files.contains(f) || !is_binary(f))
        .collect();
    included.sort();

    if included.is_empty() && !cli.quiet {
        eprintln!("warn: no files matched; writing empty archive");
    }

    let output = match cli.output {
        Some(o) => o,
        None => {
            let ts = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
            PathBuf::from(format!("backup-{ts}.tar.zst"))
        }
    };

    write_archive(&output, &included, &home, cli.level, cli.quiet, cli.verbose)?;

    if !cli.quiet {
        let size = std::fs::metadata(&output)?.len();
        println!(
            "backed up {} files -> {} ({} bytes)",
            included.len(),
            output.display(),
            size
        );
    }
    Ok(())
}

fn add_path(p: &Path, files: &mut HashSet<PathBuf>, follow_symlinks: bool) -> Result<()> {
    let meta = std::fs::symlink_metadata(p)
        .with_context(|| format!("stat {}", p.display()))?;

    if meta.file_type().is_dir() {
        for e in WalkDir::new(p)
            .follow_links(follow_symlinks)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let ft = e.file_type();
            if ft.is_file() || ft.is_symlink() {
                files.insert(e.path().to_path_buf());
            }
        }
    } else if meta.file_type().is_file() || meta.file_type().is_symlink() {
        files.insert(p.to_path_buf());
    }
    Ok(())
}

fn is_binary(f: &Path) -> bool {
    let meta = match std::fs::symlink_metadata(f) {
        Ok(m) => m,
        Err(_) => return false,
    };
    if meta.file_type().is_symlink() {
        return false;
    }
    let mut file = match std::fs::File::open(f) {
        Ok(f) => f,
        Err(_) => return false,
    };
    let mut buf = [0u8; 512];
    match file.read(&mut buf) {
        Ok(n) if n > 0 => buf[..n].contains(&0),
        _ => false,
    }
}

fn is_excluded(f: &Path, set: &globset::GlobSet) -> bool {    if set.is_match(f) {
        return true;
    }
    let mut cur = f.parent();
    while let Some(p) = cur {
        if set.is_match(p) {
            return true;
        }
        cur = p.parent();
    }
    false
}

fn archive_name_for(f: &Path, home: &Path) -> String {
    let rel = if let Ok(s) = f.strip_prefix(home) {
        s
    } else {
        f.strip_prefix("/").unwrap_or(f)
    };
    let s = rel.to_string_lossy().into_owned();
    if s.is_empty() {
        ".".to_string()
    } else {
        s
    }
}

fn write_archive(
    output: &Path,
    files: &[PathBuf],
    home: &Path,
    level: i32,
    quiet: bool,
    verbose: bool,
) -> Result<()> {
    let file = std::fs::File::create(output)
        .with_context(|| format!("creating output {}", output.display()))?;
    let enc = zstd::stream::write::Encoder::new(file, level)
        .map_err(|e| anyhow!("zstd encoder: {e}"))?;
    let mut builder = tar::Builder::new(enc);

    let total = files.len() as u64;
    let pb = if quiet || total == 0 {
        None
    } else if std::io::IsTerminal::is_terminal(&std::io::stdout()) {
        let bar = indicatif::ProgressBar::new(total);
        bar.set_draw_target(indicatif::ProgressDrawTarget::stdout());
        bar.set_style(
            indicatif::ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} {msg}")
                .unwrap_or_else(|_| indicatif::ProgressStyle::default_bar()),
        );
        Some(bar)
    } else {
        None
    };

    for (idx, f) in files.iter().enumerate() {
        let archive_name = archive_name_for(f, home);
        if verbose && !quiet {
            let line = format!("[{}/{}] {}", idx + 1, total, archive_name);
            if let Some(pb) = &pb {
                pb.println(line);
            } else {
                println!("{line}");
            }
        }
        if let Some(pb) = &pb {
            pb.set_message(archive_name.clone());
        }
        let meta = std::fs::symlink_metadata(f)
            .with_context(|| format!("stat {}", f.display()))?;
        if meta.file_type().is_symlink() {
            let target = std::fs::read_link(f)
                .with_context(|| format!("read_link {}", f.display()))?;
            let mut header = tar::Header::new_gnu();
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_size(0);
            builder
                .append_link(&mut header, &archive_name, &target)
                .with_context(|| format!("adding {} as {}", f.display(), archive_name))?;
        } else {
            builder
                .append_path_with_name(f, &archive_name)
                .with_context(|| format!("adding {} as {}", f.display(), archive_name))?;
        }
        if let Some(pb) = &pb {
            pb.inc(1);
        }
    }

    if let Some(pb) = pb {
        pb.finish_and_clear();
    }

    builder.finish().map_err(|e| anyhow!("tar finish: {e}"))?;
    let enc = builder.into_inner().map_err(|e| anyhow!("into_inner: {e}"))?;
    enc.finish().map_err(|e| anyhow!("zstd finish: {e}"))?;
    Ok(())
}
