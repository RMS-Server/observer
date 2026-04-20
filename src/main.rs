mod cli;
mod core;
mod export;
mod runner;
mod scenario;
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
         If no scenario names are given, runs `selected_scenarios` from config.\n"
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
        "-h" | "--help" => usage(),
        _ => {
            eprintln!("unknown command: {}", raw[0]);
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
