use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::process::{ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    En,
    Zh,
}

fn detect_lang() -> Lang {
    let env = std::env::var("LC_ALL")
        .or_else(|_| std::env::var("LANG"))
        .unwrap_or_default()
        .to_lowercase();
    if env.starts_with("zh") {
        Lang::Zh
    } else {
        Lang::En
    }
}

impl Default for Lang {
    fn default() -> Self {
        detect_lang()
    }
}

fn default_lang() -> Lang {
    detect_lang()
}

#[derive(Serialize, Deserialize, Clone, Default, Debug)]
pub struct Config {
    #[serde(default)]
    pub server_dir: Option<String>,
    #[serde(default)]
    pub server_cmd: Vec<String>,
    #[serde(default)]
    pub rules: Vec<RuleDef>,
    #[serde(default = "default_lang")]
    pub lang: Lang,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum MatchMode {
    Contains,
    Exact,
    Glob,
    Regex,
}

fn default_match_mode() -> MatchMode {
    MatchMode::Regex
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RuleDef {
    pub pattern: String,
    pub commands: Vec<String>,
    #[serde(default)]
    pub once: bool,
    #[serde(default)]
    pub delay_ms: u64,
    #[serde(default)]
    pub gap_ms: u64,
    #[serde(default = "default_match_mode", rename = "match")]
    pub match_mode: MatchMode,
}

impl Default for RuleDef {
    fn default() -> Self {
        Self {
            pattern: String::new(),
            commands: Vec::new(),
            once: false,
            delay_ms: 0,
            gap_ms: 0,
            match_mode: MatchMode::Contains,
        }
    }
}

struct CompiledRule {
    re: Regex,
    def: RuleDef,
    fired: bool,
}

pub fn validate_rule(def: &RuleDef) -> Result<(), String> {
    build_matcher(def).map(|_| ())
}

pub fn validate_rules(defs: &[RuleDef]) -> Result<(), String> {
    for d in defs {
        validate_rule(d)?;
    }
    Ok(())
}

fn build_matcher(def: &RuleDef) -> Result<Regex, String> {
    let pat = match def.match_mode {
        MatchMode::Contains => format!("(?i){}", regex::escape(&def.pattern)),
        MatchMode::Exact => format!("^{}$", regex::escape(&def.pattern)),
        MatchMode::Glob => {
            let esc = regex::escape(&def.pattern);
            let converted = esc.replace(r"\*", ".*").replace(r"\?", ".");
            format!("^{}$", converted)
        }
        MatchMode::Regex => def.pattern.clone(),
    };
    Regex::new(&pat).map_err(|e| {
        format!(
            "bad {:?} pattern {:?}: {e}",
            def.match_mode, def.pattern
        )
    })
}

fn build_compiled(defs: &[RuleDef]) -> Result<Vec<CompiledRule>, String> {
    defs.iter()
        .map(|d| {
            build_matcher(d).map(|re| CompiledRule {
                re,
                def: d.clone(),
                fired: false,
            })
        })
        .collect()
}

#[derive(Debug, Clone)]
pub enum Event {
    Stdout(String),
    Stderr(String),
    RuleMatch {
        pattern: String,
        count: usize,
        delay_ms: u64,
        gap_ms: u64,
    },
    RuleSent(String),
    Exited(Option<i32>),
}

type Writer = Arc<Mutex<Option<ChildStdin>>>;

pub struct Session {
    writer: Writer,
}

impl Session {
    pub fn spawn(config: &Config) -> io::Result<(Self, mpsc::Receiver<Event>)> {
        if config.server_cmd.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "server_cmd is empty",
            ));
        }
        let rules = build_compiled(&config.rules)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let rules = Arc::new(Mutex::new(rules));

        let mut cmd = Command::new(&config.server_cmd[0]);
        cmd.args(&config.server_cmd[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(dir) = &config.server_dir {
            if !dir.is_empty() {
                cmd.current_dir(dir);
            }
        }
        let mut child = cmd.spawn()?;

        let writer: Writer = Arc::new(Mutex::new(Some(child.stdin.take().expect("stdin"))));
        let stdout = child.stdout.take().expect("stdout");
        let stderr = child.stderr.take().expect("stderr");

        let (ev_tx, ev_rx) = mpsc::channel();

        {
            let tx = ev_tx.clone();
            let rules = Arc::clone(&rules);
            let writer = Arc::clone(&writer);
            thread::spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines().map_while(Result::ok) {
                    let _ = tx.send(Event::Stdout(line.clone()));
                    evaluate(&rules, &line, &writer, &tx);
                }
            });
        }
        {
            let tx = ev_tx.clone();
            let rules = Arc::clone(&rules);
            let writer = Arc::clone(&writer);
            thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines().map_while(Result::ok) {
                    let _ = tx.send(Event::Stderr(line.clone()));
                    evaluate(&rules, &line, &writer, &tx);
                }
            });
        }
        {
            let tx = ev_tx.clone();
            thread::spawn(move || {
                let status = child.wait();
                let code = status.ok().and_then(|s| s.code());
                let _ = tx.send(Event::Exited(code));
            });
        }

        Ok((Session { writer }, ev_rx))
    }

    pub fn send_cmd(&self, cmd: &str) -> bool {
        write_line(&self.writer, cmd)
    }

    pub fn close_stdin(&self) {
        self.writer.lock().unwrap().take();
    }
}

fn write_line(writer: &Writer, cmd: &str) -> bool {
    let mut g = writer.lock().unwrap();
    let Some(w) = g.as_mut() else {
        return false;
    };
    if w.write_all(cmd.as_bytes()).is_err() {
        return false;
    }
    if !cmd.ends_with('\n') && w.write_all(b"\n").is_err() {
        return false;
    }
    let _ = w.flush();
    true
}

fn evaluate(
    rules: &Arc<Mutex<Vec<CompiledRule>>>,
    line: &str,
    writer: &Writer,
    tx: &mpsc::Sender<Event>,
) {
    let triggered: Vec<(String, Vec<String>, u64, u64)> = {
        let mut g = rules.lock().unwrap();
        let mut out = Vec::new();
        for r in g.iter_mut() {
            if r.def.once && r.fired {
                continue;
            }
            if r.re.is_match(line) {
                out.push((
                    r.def.pattern.clone(),
                    r.def.commands.clone(),
                    r.def.delay_ms,
                    r.def.gap_ms,
                ));
                if r.def.once {
                    r.fired = true;
                }
            }
        }
        out
    };

    for (pattern, cmds, delay_ms, gap_ms) in triggered {
        let writer = Arc::clone(writer);
        let tx = tx.clone();
        thread::spawn(move || {
            let _ = tx.send(Event::RuleMatch {
                pattern,
                count: cmds.len(),
                delay_ms,
                gap_ms,
            });
            if delay_ms > 0 {
                thread::sleep(Duration::from_millis(delay_ms));
            }
            for (i, c) in cmds.into_iter().enumerate() {
                if i > 0 && gap_ms > 0 {
                    thread::sleep(Duration::from_millis(gap_ms));
                }
                if !write_line(&writer, &c) {
                    break;
                }
                let _ = tx.send(Event::RuleSent(c));
            }
        });
    }
}

pub fn load_config(path: &str) -> io::Result<Config> {
    let text = fs::read_to_string(path)?;
    let cfg: Config = serde_json::from_str(&text)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("config parse: {e}")))?;
    validate_rules(&cfg.rules).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(cfg)
}

pub fn save_config(path: &str, config: &Config) -> io::Result<()> {
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("{e}")))?;
    fs::write(path, json)
}
