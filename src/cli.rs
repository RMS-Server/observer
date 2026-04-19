use std::io::{self, BufRead, Write};
use std::thread;
use std::time::Duration;

use crate::core::{Config, Event, Session, load_config};

pub fn run(config_path: Option<&str>, server_cmd_override: Vec<String>) -> io::Result<()> {
    let mut config = match config_path {
        Some(p) => load_config(p)?,
        None => Config::default(),
    };
    if !server_cmd_override.is_empty() {
        config.server_cmd = server_cmd_override;
    }
    if config.server_cmd.is_empty() {
        eprintln!("[observer] no server_cmd — set it in config or pass after --");
        std::process::exit(2);
    }

    let (session, events) = Session::spawn(&config)?;
    let rule_count = config.rules.len();

    thread::spawn(move || {
        let stdout = io::stdout();
        for ev in events.iter() {
            let mut out = stdout.lock();
            match ev {
                Event::Stdout(l) => {
                    let _ = writeln!(out, "[OUT] {l}");
                }
                Event::Stderr(l) => {
                    let _ = writeln!(out, "[ERR] {l}");
                }
                Event::RuleMatch {
                    pattern,
                    count,
                    delay_ms,
                    gap_ms,
                } => {
                    let _ = writeln!(
                        out,
                        "[RULE] match /{pattern}/ -> {count} cmd(s) (delay {delay_ms}ms, gap {gap_ms}ms)"
                    );
                }
                Event::RuleSent(c) => {
                    let _ = writeln!(out, "[RULE] sent: {c}");
                }
                Event::Exited(code) => {
                    let _ = writeln!(
                        out,
                        "[observer] server exited: code={:?}",
                        code.unwrap_or(-1)
                    );
                    let _ = out.flush();
                    thread::sleep(Duration::from_millis(50));
                    std::process::exit(code.unwrap_or(0));
                }
            }
            let _ = out.flush();
        }
    });

    eprintln!(
        "[observer] loaded {rule_count} rule(s). type server commands; `:quit` closes server stdin; Ctrl-C kills observer."
    );

    let stdin = io::stdin();
    let mut input = String::new();
    loop {
        input.clear();
        match stdin.lock().read_line(&mut input) {
            Ok(0) => {
                eprintln!("[observer] user stdin closed; server keeps running (rules still active)");
                break;
            }
            Ok(_) => {
                if input.trim() == ":quit" {
                    eprintln!("[observer] :quit -> closing server stdin");
                    session.close_stdin();
                    break;
                }
                if !session.send_cmd(&input) {
                    eprintln!("[observer] server stdin unavailable");
                    break;
                }
            }
            Err(e) => {
                eprintln!("[observer] stdin read failed: {e}");
                break;
            }
        }
    }

    loop {
        thread::sleep(Duration::from_secs(3600));
    }
}
