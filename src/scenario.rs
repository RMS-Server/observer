use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use regex::Regex;

#[derive(Debug, Clone)]
pub struct Pattern {
    pub regex: Regex,
    pub raw: String,
}

impl Pattern {
    pub fn compile(src: &str) -> Result<Self, String> {
        let trimmed = src.trim();
        if trimmed.is_empty() {
            return Err("pattern is empty".into());
        }
        let regex = if let Some(re_src) = trimmed.strip_prefix("re:") {
            let re_src = re_src.trim_start();
            if re_src.is_empty() {
                return Err("empty regex after `re:`".into());
            }
            Regex::new(re_src).map_err(|e| format!("bad regex {re_src:?}: {e}"))?
        } else {
            let placeholders = trimmed.matches("{}").count();
            if placeholders > 1 {
                return Err("at most one {} placeholder per pattern".into());
            }
            let pat = if placeholders == 1 {
                let (a, b) = trimmed.split_once("{}").unwrap();
                format!(
                    "{}(-?\\d+(?:\\.\\d+)?){}",
                    regex::escape(a),
                    regex::escape(b)
                )
            } else {
                regex::escape(trimmed)
            };
            Regex::new(&pat).map_err(|e| format!("internal regex build failed: {e}"))?
        };
        Ok(Pattern {
            regex,
            raw: trimmed.to_string(),
        })
    }
}

#[derive(Debug, Clone)]
pub enum Step {
    Start,
    Wait {
        timeout: Duration,
        pattern: Pattern,
    },
    Send(String),
    Sleep(Duration),
    Grab {
        name: String,
        pattern: Pattern,
    },
    Loop {
        total: Duration,
        every: Duration,
        body: Vec<Step>,
    },
    Stop,
}

#[derive(Debug, Clone)]
pub struct Scenario {
    pub name: String,
    #[allow(dead_code)] // surfaced to callers for diagnostics
    pub path: PathBuf,
    pub steps: Vec<Step>,
}

#[derive(Debug, Clone)]
pub struct ParseError {
    pub line: usize,
    pub msg: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.msg)
    }
}

impl std::error::Error for ParseError {}

const DEFAULT_WAIT_TIMEOUT: Duration = Duration::from_secs(120);

pub fn parse(src: &str, path: &Path) -> Result<Scenario, ParseError> {
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("scenario")
        .to_string();

    let mut top: Vec<Step> = Vec::new();
    let mut stack: Vec<(LoopHeader, Vec<Step>, usize)> = Vec::new();

    for (idx, raw_line) in src.lines().enumerate() {
        let lineno = idx + 1;
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        let (verb, rest) = split_verb(line);
        match verb {
            "end" => {
                if !rest.is_empty() {
                    return Err(err(lineno, "`end` takes no arguments"));
                }
                let (hdr, body, _hdr_line) = stack
                    .pop()
                    .ok_or_else(|| err(lineno, "`end` without matching `loop`"))?;
                let step = Step::Loop {
                    total: hdr.total,
                    every: hdr.every,
                    body,
                };
                push_step(&mut stack, &mut top, step);
            }
            "wait" => {
                let (timeout, pattern_src) = parse_wait_args(rest, lineno)?;
                let pattern = Pattern::compile(pattern_src).map_err(|m| err(lineno, &m))?;
                push_step(
                    &mut stack,
                    &mut top,
                    Step::Wait { timeout, pattern },
                );
            }
            "send" => {
                if rest.is_empty() {
                    return Err(err(lineno, "`send` requires a command"));
                }
                push_step(&mut stack, &mut top, Step::Send(rest.to_string()));
            }
            "sleep" => {
                let dur = parse_duration(rest).map_err(|m| err(lineno, &m))?;
                push_step(&mut stack, &mut top, Step::Sleep(dur));
            }
            "grab" => {
                let (name, pattern_src) = split_verb(rest);
                if name.is_empty() {
                    return Err(err(lineno, "`grab` requires a metric name"));
                }
                if pattern_src.is_empty() {
                    return Err(err(lineno, "`grab` requires a pattern"));
                }
                if !is_ident(name) {
                    return Err(err(
                        lineno,
                        &format!("metric name {name:?} must be [A-Za-z_][A-Za-z0-9_]*"),
                    ));
                }
                let pattern = Pattern::compile(pattern_src).map_err(|m| err(lineno, &m))?;
                if pattern.regex.captures_len() < 2 {
                    return Err(err(
                        lineno,
                        "`grab` pattern must include {} or a regex capture group",
                    ));
                }
                push_step(
                    &mut stack,
                    &mut top,
                    Step::Grab {
                        name: name.to_string(),
                        pattern,
                    },
                );
            }
            "loop" => {
                let hdr = parse_loop_header(rest, lineno)?;
                stack.push((hdr, Vec::new(), lineno));
            }
            "stop" => {
                if !rest.is_empty() {
                    return Err(err(lineno, "`stop` takes no arguments"));
                }
                push_step(&mut stack, &mut top, Step::Stop);
            }
            "start" => {
                if !rest.is_empty() {
                    return Err(err(lineno, "`start` takes no arguments"));
                }
                push_step(&mut stack, &mut top, Step::Start);
            }
            _ => return Err(err(lineno, &format!("unknown verb {verb:?}"))),
        }
    }

    if let Some((_, _, ln)) = stack.last() {
        return Err(err(*ln, "`loop` missing matching `end`"));
    }

    Ok(Scenario {
        name,
        path: path.to_path_buf(),
        steps: top,
    })
}

#[derive(Debug)]
struct LoopHeader {
    total: Duration,
    every: Duration,
}

fn parse_loop_header(rest: &str, lineno: usize) -> Result<LoopHeader, ParseError> {
    // syntax: <total> every <interval>
    let (total_str, tail) = split_verb(rest);
    if total_str.is_empty() {
        return Err(err(lineno, "`loop` requires <total> every <interval>"));
    }
    let total = parse_duration(total_str).map_err(|m| err(lineno, &m))?;
    let (kw, interval_str) = split_verb(tail);
    if kw != "every" {
        return Err(err(
            lineno,
            "`loop` syntax: `loop <total> every <interval>`",
        ));
    }
    let every = parse_duration(interval_str).map_err(|m| err(lineno, &m))?;
    if every.is_zero() {
        return Err(err(lineno, "loop interval must be > 0"));
    }
    Ok(LoopHeader { total, every })
}

fn parse_wait_args(rest: &str, lineno: usize) -> Result<(Duration, &str), ParseError> {
    if rest.is_empty() {
        return Err(err(lineno, "`wait` requires a pattern"));
    }
    // Optional leading duration — only consumed if it parses.
    let (head, tail) = split_verb(rest);
    if !head.is_empty() && tail.is_empty() {
        // single token after verb — that's the pattern, not a timeout
        return Ok((DEFAULT_WAIT_TIMEOUT, rest));
    }
    if let Ok(d) = parse_duration(head) {
        Ok((d, tail))
    } else {
        Ok((DEFAULT_WAIT_TIMEOUT, rest))
    }
}

fn push_step(
    stack: &mut [(LoopHeader, Vec<Step>, usize)],
    top: &mut Vec<Step>,
    step: Step,
) {
    match stack.last_mut() {
        Some((_, body, _)) => body.push(step),
        None => top.push(step),
    }
}

fn err(line: usize, msg: &str) -> ParseError {
    ParseError {
        line,
        msg: msg.to_string(),
    }
}

fn strip_comment(line: &str) -> &str {
    match line.find('#') {
        Some(i) => &line[..i],
        None => line,
    }
}

fn split_verb(s: &str) -> (&str, &str) {
    let s = s.trim_start();
    match s.find(|c: char| c.is_whitespace()) {
        Some(i) => (&s[..i], s[i..].trim_start()),
        None => (s, ""),
    }
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

pub fn parse_duration(src: &str) -> Result<Duration, String> {
    let s = src.trim();
    if s.is_empty() {
        return Err("duration missing".into());
    }
    let (num_part, unit) = split_num_unit(s)?;
    let n: u64 = num_part
        .parse()
        .map_err(|_| format!("bad duration number {num_part:?}"))?;
    let d = match unit {
        "ms" => Duration::from_millis(n),
        "s" => Duration::from_secs(n),
        "m" => Duration::from_secs(n * 60),
        other => return Err(format!("bad duration unit {other:?} (use ms/s/m)")),
    };
    Ok(d)
}

fn split_num_unit(s: &str) -> Result<(&str, &str), String> {
    let pos = s
        .find(|c: char| !c.is_ascii_digit())
        .ok_or_else(|| format!("missing unit in {s:?} (use ms/s/m)"))?;
    if pos == 0 {
        return Err(format!("bad duration {s:?}"));
    }
    Ok((&s[..pos], &s[pos..]))
}

pub fn load_scenarios(dir: &Path) -> Vec<Result<Scenario, ScenarioLoadError>> {
    let mut out = Vec::new();
    let Ok(rd) = fs::read_dir(dir) else {
        return out;
    };
    let mut entries: Vec<PathBuf> = rd
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("mcb"))
        .collect();
    entries.sort();
    for path in entries {
        out.push(load_scenario_file(&path));
    }
    out
}

#[derive(Debug)]
pub struct ScenarioLoadError {
    pub path: PathBuf,
    pub msg: String,
}

impl std::fmt::Display for ScenarioLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.msg)
    }
}

pub fn load_scenario_file(path: &Path) -> Result<Scenario, ScenarioLoadError> {
    let text = fs::read_to_string(path).map_err(|e| ScenarioLoadError {
        path: path.to_path_buf(),
        msg: format!("read failed: {e}"),
    })?;
    parse(&text, path).map_err(|e| ScenarioLoadError {
        path: path.to_path_buf(),
        msg: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_literal_no_escape() {
        let p = Pattern::compile("Done (").unwrap();
        assert!(p.regex.is_match("[12:34:56] Done (1.234s)"));
    }

    #[test]
    fn pattern_placeholder_grabs_number() {
        let p = Pattern::compile("Average: {} ms").unwrap();
        let caps = p.regex.captures("Average: 12.34 ms").unwrap();
        assert_eq!(caps.get(1).unwrap().as_str(), "12.34");
    }

    #[test]
    fn pattern_re_prefix() {
        let p = Pattern::compile("re:^hello [a-z]+").unwrap();
        assert!(p.regex.is_match("hello world"));
        assert!(!p.regex.is_match("HELLO world"));
    }

    #[test]
    fn reject_multi_placeholder() {
        assert!(Pattern::compile("{} and {}").is_err());
    }

    #[test]
    fn parse_simple() {
        let src = r#"
# comment
start
wait Done (
send /tick rate 20
sleep 2s
stop
"#;
        let s = parse(src, Path::new("x.mcb")).unwrap();
        assert_eq!(s.steps.len(), 5);
        assert!(matches!(s.steps[0], Step::Start));
        assert!(matches!(s.steps[1], Step::Wait { .. }));
        assert!(matches!(s.steps[2], Step::Send(_)));
        assert!(matches!(s.steps[3], Step::Sleep(_)));
        assert!(matches!(s.steps[4], Step::Stop));
    }

    #[test]
    fn parse_start_no_args() {
        let src = "start extra\n";
        let e = parse(src, Path::new("x.mcb")).unwrap_err();
        assert_eq!(e.line, 1);
    }

    #[test]
    fn grab_without_capture_rejected() {
        let src = "grab tps re:ready\n";
        let e = parse(src, Path::new("x.mcb")).unwrap_err();
        assert_eq!(e.line, 1);
    }

    #[test]
    fn parse_loop() {
        let src = "loop 30s every 1s\n  send /tick query rate\n  grab mspt Average: {} ms\nend\n";
        let s = parse(src, Path::new("y.mcb")).unwrap();
        assert_eq!(s.steps.len(), 1);
        match &s.steps[0] {
            Step::Loop { total, every, body } => {
                assert_eq!(*total, Duration::from_secs(30));
                assert_eq!(*every, Duration::from_secs(1));
                assert_eq!(body.len(), 2);
            }
            _ => panic!("expected Loop"),
        }
    }

    #[test]
    fn parse_wait_with_timeout() {
        let src = "wait 60s Done (\n";
        let s = parse(src, Path::new("z.mcb")).unwrap();
        match &s.steps[0] {
            Step::Wait { timeout, .. } => assert_eq!(*timeout, Duration::from_secs(60)),
            _ => panic!(),
        }
    }

    #[test]
    fn parse_wait_no_timeout() {
        let src = "wait Done (\n";
        let s = parse(src, Path::new("z.mcb")).unwrap();
        match &s.steps[0] {
            Step::Wait { timeout, .. } => assert_eq!(*timeout, DEFAULT_WAIT_TIMEOUT),
            _ => panic!(),
        }
    }

    #[test]
    fn parse_errors_report_line() {
        let src = "send hi\nwtf x\n";
        let e = parse(src, Path::new("e.mcb")).unwrap_err();
        assert_eq!(e.line, 2);
    }

    #[test]
    fn parse_end_without_loop() {
        let src = "end\n";
        let e = parse(src, Path::new("e.mcb")).unwrap_err();
        assert_eq!(e.line, 1);
    }

    #[test]
    fn parse_loop_missing_end() {
        let src = "loop 10s every 1s\n  send x\n";
        let e = parse(src, Path::new("e.mcb")).unwrap_err();
        assert_eq!(e.line, 1);
    }

    #[test]
    fn duration_units() {
        assert_eq!(parse_duration("500ms").unwrap(), Duration::from_millis(500));
        assert_eq!(parse_duration("2s").unwrap(), Duration::from_secs(2));
        assert_eq!(parse_duration("3m").unwrap(), Duration::from_secs(180));
        assert!(parse_duration("10").is_err());
        assert!(parse_duration("xs").is_err());
    }
}
