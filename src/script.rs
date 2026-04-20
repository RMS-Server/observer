use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::core::{Config, load_config};
use crate::scenario;

const DEFAULT_CFG: &str = "./observer.json";

const TEMPLATE: &str = "# Scenario: launch server, wait for ready, sample MSPT, stop.
# Pattern syntax:
#   literal text (e.g. `Done (`)
#   `{}`           — placeholder that grabs a decimal number
#   `re:<regex>`   — full regex with capture group

start
wait Done (
send /tick rate 20
sleep 2s

loop 30s every 1s
    send /tick query rate
    grab mspt Average tick time: {} ms
end

stop
";

pub fn run_init(cfg_path: Option<&str>, name: &str) -> io::Result<()> {
    if name.is_empty() {
        eprintln!("[observer] s init: missing <name>");
        std::process::exit(2);
    }
    if name.contains('/') || name.contains('\\') {
        eprintln!("[observer] s init: <name> must not contain path separators");
        std::process::exit(2);
    }
    let cfg = load_or_default(cfg_path);
    let stem = name.trim_end_matches(".mcb");
    let dir = &cfg.scenarios_dir;
    fs::create_dir_all(dir)?;
    let path = dir.join(format!("{stem}.mcb"));
    if path.exists() {
        eprintln!("[observer] s init: {} already exists", path.display());
        std::process::exit(1);
    }
    fs::write(&path, TEMPLATE)?;
    eprintln!("[observer] created {}", path.display());
    Ok(())
}

pub fn run_format(cfg_path: Option<&str>, target: &str) -> io::Result<()> {
    let path = resolve_target(cfg_path, target);
    let src = fs::read_to_string(&path).map_err(|e| {
        io::Error::new(e.kind(), format!("read {}: {e}", path.display()))
    })?;
    let formatted = match format_text(&src, &path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("[observer] s format: {}: {e}", path.display());
            std::process::exit(1);
        }
    };
    if formatted == src {
        eprintln!("[observer] {} already formatted", path.display());
        return Ok(());
    }
    fs::write(&path, &formatted)?;
    eprintln!("[observer] formatted {}", path.display());
    Ok(())
}

pub fn run_check(cfg_path: Option<&str>, target: &str) -> io::Result<()> {
    let path = resolve_target(cfg_path, target);
    let src = fs::read_to_string(&path).map_err(|e| {
        io::Error::new(e.kind(), format!("read {}: {e}", path.display()))
    })?;
    match scenario::parse(&src, &path) {
        Ok(s) => {
            eprintln!(
                "[observer] ok: {} ({} top-level step(s))",
                path.display(),
                s.steps.len()
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("[observer] {}: {e}", path.display());
            std::process::exit(1);
        }
    }
}

fn load_or_default(cfg_path: Option<&str>) -> Config {
    let path = cfg_path.unwrap_or(DEFAULT_CFG);
    load_config(path).unwrap_or_default()
}

fn resolve_target(cfg_path: Option<&str>, target: &str) -> PathBuf {
    let direct = Path::new(target);
    if direct.is_file() {
        return direct.to_path_buf();
    }
    let cfg = load_or_default(cfg_path);
    let stem = target.trim_end_matches(".mcb");
    cfg.scenarios_dir.join(format!("{stem}.mcb"))
}

/// Normalize indentation and whitespace in a scenario script.
///
/// Rules:
/// - validate by parsing first; reject invalid input
/// - 4 spaces per `loop` depth
/// - single space between verb and args, args kept verbatim after trimming
/// - blank lines preserved; comment-only lines indented at current depth
/// - inline comments separated by two spaces
fn format_text(src: &str, path: &Path) -> Result<String, String> {
    scenario::parse(src, path).map_err(|e| e.to_string())?;

    let mut out = String::new();
    let mut depth: usize = 0;
    for raw in src.lines() {
        let (code, comment) = split_comment(raw);
        let code = code.trim();
        if code.is_empty() {
            if let Some(c) = comment {
                push_indent(&mut out, depth);
                out.push_str(c.trim_end());
            }
            out.push('\n');
            continue;
        }
        let (verb, rest) = split_verb(code);
        let emit_depth = if verb == "end" { depth.saturating_sub(1) } else { depth };
        push_indent(&mut out, emit_depth);
        out.push_str(verb);
        let rest = rest.trim();
        if !rest.is_empty() {
            out.push(' ');
            out.push_str(rest);
        }
        if let Some(c) = comment {
            out.push_str("  ");
            out.push_str(c.trim_end());
        }
        out.push('\n');
        match verb {
            "loop" => depth += 1,
            "end" => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    Ok(out)
}

fn push_indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("    ");
    }
}

fn split_comment(s: &str) -> (&str, Option<&str>) {
    match s.find('#') {
        Some(i) => (&s[..i], Some(&s[i..])),
        None => (s, None),
    }
}

fn split_verb(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    match s.find(|c: char| c.is_whitespace()) {
        Some(i) => (&s[..i], s[i..].trim_start()),
        None => (s, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_collapses_inner_whitespace() {
        let src = "wait   Done (\nsend   /tick rate 20\n";
        let got = format_text(src, Path::new("x.mcb")).unwrap();
        assert_eq!(got, "wait Done (\nsend /tick rate 20\n");
    }

    #[test]
    fn format_indents_loop_body() {
        let src = "loop 10s every 1s\nsend x\ngrab m Average: {} ms\nend\n";
        let got = format_text(src, Path::new("x.mcb")).unwrap();
        let want = "loop 10s every 1s\n    send x\n    grab m Average: {} ms\nend\n";
        assert_eq!(got, want);
    }

    #[test]
    fn format_preserves_comments_and_blank_lines() {
        let src = "# header\n\nstart   # go\nstop\n";
        let got = format_text(src, Path::new("x.mcb")).unwrap();
        let want = "# header\n\nstart  # go\nstop\n";
        assert_eq!(got, want);
    }

    #[test]
    fn format_rejects_invalid() {
        let src = "wtf\n";
        assert!(format_text(src, Path::new("x.mcb")).is_err());
    }

    #[test]
    fn format_idempotent() {
        let src = "loop 10s every 1s\n    send x\nend\n";
        let once = format_text(src, Path::new("x.mcb")).unwrap();
        let twice = format_text(&once, Path::new("x.mcb")).unwrap();
        assert_eq!(once, twice);
    }
}
