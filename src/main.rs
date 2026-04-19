mod cli;
mod core;
mod strings;
mod tui;

use std::env;
use std::io;

fn usage() -> ! {
    eprintln!(
        "observer — MC server wrapper with rule engine\n\n\
         TUI mode (default):\n\
           observer\n\
           observer tui [--config <path>]\n\n\
         Headless mode:\n\
           observer run [--config <path>] [-- <server-cmd> [args...]]\n\
           observer [--config <path>] -- <server-cmd> [args...]\n\
           observer <server-cmd> [args...]\n"
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
            let rest = &raw[1..];
            let mut cfg = None;
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
                    _ => {
                        eprintln!("unknown tui arg: {}", rest[i]);
                        usage();
                    }
                }
                i += 1;
            }
            tui::run(cfg.as_deref())
        }
        "run" => dispatch_headless(&raw[1..]),
        "-h" | "--help" => usage(),
        _ => dispatch_headless(&raw),
    }
}

fn dispatch_headless(args: &[String]) -> io::Result<()> {
    let sep = args.iter().position(|a| a == "--");
    let (wrapper, server): (&[String], Vec<String>) = match sep {
        Some(i) => (&args[..i], args[i + 1..].to_vec()),
        None => (&[][..], args.to_vec()),
    };

    let mut cfg = None;
    let mut i = 0;
    while i < wrapper.len() {
        match wrapper[i].as_str() {
            "-c" | "--config" => {
                i += 1;
                if i >= wrapper.len() {
                    eprintln!("--config needs a path");
                    usage();
                }
                cfg = Some(wrapper[i].clone());
            }
            "-h" | "--help" => usage(),
            other => {
                eprintln!("unknown flag: {other}");
                usage();
            }
        }
        i += 1;
    }
    cli::run(cfg.as_deref(), server)
}
