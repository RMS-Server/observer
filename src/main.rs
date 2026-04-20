mod cli;
mod core;
mod export;
mod runner;
mod scenario;
mod script;
mod strings;
mod tui;

use std::env;
use std::io;

fn usage() -> ! {
    eprintln!(
        "observer — scripted MC benchmark runner\n\n\
         TUI mode (default):\n\
           observer\n\
           observer tui [--config <path>]\n\n\
         Headless run (executes .mcb scenarios, exports CSV+JSON, exits):\n\
           observer run [--config <path>] [scenario-name ...]\n\n\
         Scenario script tools:\n\
           observer s init   <name>              create a new <name>.mcb in scenarios_dir\n\
           observer s format <name-or-path>      normalize indentation/whitespace\n\
           observer s check  <name-or-path>      parse and report syntax errors\n\
         (all accept `-c/--config <path>`)\n\n\
         If no scenario names are given to `run`, runs `selected_scenarios` from config.\n"
    );
    std::process::exit(2);
}

fn main() -> io::Result<()> {
    let raw: Vec<String> = env::args().skip(1).collect();

    if raw.is_empty() {
        return tui::run(None);
    }
    match raw[0].as_str() {
        "tui" => {
            let cfg = parse_config_flag(&raw[1..]);
            tui::run(cfg.as_deref())
        }
        "run" => {
            let rest = &raw[1..];
            let mut cfg: Option<String> = None;
            let mut scenarios: Vec<String> = Vec::new();
            let mut i = 0;
            while i < rest.len() {
                match rest[i].as_str() {
                    "-c" | "--config" => {
                        i += 1;
                        if i >= rest.len() {
                            eprintln!("--config needs a path");
                            usage();
                        }
                        cfg = Some(rest[i].clone());
                    }
                    "-h" | "--help" => usage(),
                    other if other.starts_with('-') => {
                        eprintln!("unknown flag: {other}");
                        usage();
                    }
                    _ => scenarios.push(rest[i].clone()),
                }
                i += 1;
            }
            cli::run_headless(cfg.as_deref(), scenarios)
        }
        "s" => run_script_cmd(&raw[1..]),
        "-h" | "--help" => usage(),
        _ => {
            eprintln!("unknown command: {}", raw[0]);
            usage();
        }
    }
}

fn run_script_cmd(args: &[String]) -> io::Result<()> {
    if args.is_empty() {
        eprintln!("s: expected subcommand (init|format|check)");
        usage();
    }
    let sub = args[0].as_str();
    let rest = &args[1..];

    let mut cfg: Option<String> = None;
    let mut positional: Vec<String> = Vec::new();
    let mut i = 0;
    while i < rest.len() {
        match rest[i].as_str() {
            "-c" | "--config" => {
                i += 1;
                if i >= rest.len() {
                    eprintln!("--config needs a path");
                    usage();
                }
                cfg = Some(rest[i].clone());
            }
            "-h" | "--help" => usage(),
            other if other.starts_with('-') => {
                eprintln!("unknown flag: {other}");
                usage();
            }
            _ => positional.push(rest[i].clone()),
        }
        i += 1;
    }

    match sub {
        "init" => {
            let Some(name) = positional.first() else {
                eprintln!("s init: expected <name>");
                usage();
            };
            if positional.len() > 1 {
                eprintln!("s init: unexpected extra argument {:?}", positional[1]);
                usage();
            }
            script::run_init(cfg.as_deref(), name)
        }
        "format" => {
            let Some(target) = positional.first() else {
                eprintln!("s format: expected <name-or-path>");
                usage();
            };
            if positional.len() > 1 {
                eprintln!("s format: unexpected extra argument {:?}", positional[1]);
                usage();
            }
            script::run_format(cfg.as_deref(), target)
        }
        "check" => {
            let Some(target) = positional.first() else {
                eprintln!("s check: expected <name-or-path>");
                usage();
            };
            if positional.len() > 1 {
                eprintln!("s check: unexpected extra argument {:?}", positional[1]);
                usage();
            }
            script::run_check(cfg.as_deref(), target)
        }
        other => {
            eprintln!("s: unknown subcommand {other:?}");
            usage();
        }
    }
}

fn parse_config_flag(args: &[String]) -> Option<String> {
    let mut cfg = None;
    let mut iter = args.iter();
    while let Some(a) = iter.next() {
        match a.as_str() {
            "-c" | "--config" => {
                let Some(v) = iter.next() else {
                    eprintln!("--config needs a path");
                    usage();
                };
                cfg = Some(v.clone());
            }
            "-h" | "--help" => usage(),
            other => {
                eprintln!("unknown arg: {other}");
                usage();
            }
        }
    }
    cfg
}
