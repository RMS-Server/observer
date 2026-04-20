use std::io::{self, Write};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use crate::core::{Config, Event, Sample, SessionCtrl, load_config};
use crate::export;
use crate::runner;
use crate::scenario::{self, Scenario};

const DEFAULT_CFG: &str = "./observer.json";

pub fn run_headless(config_path: Option<&str>, scenario_names: Vec<String>) -> io::Result<()> {
    let path = config_path.unwrap_or(DEFAULT_CFG);
    let config = match load_config(path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[observer] load config {path}: {e}");
            return Err(e);
        }
    };
    if config.server_cmd.is_empty() {
        eprintln!("[observer] server_cmd is empty — edit {path}");
        std::process::exit(2);
    }

    let scenarios = pick_scenarios(&config, &scenario_names)?;
    if scenarios.is_empty() {
        eprintln!(
            "[observer] no scenarios selected (pass names on CLI or set `selected_scenarios` in config)"
        );
        std::process::exit(2);
    }
    eprintln!(
        "[observer] running {} scenario(s): {}",
        scenarios.len(),
        scenarios
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );

    let (ctrl, session_events) = SessionCtrl::new(config.clone());
    let handle = runner::spawn(ctrl.clone(), scenarios.clone());

    let samples: Vec<Sample> = {
        let mut collected: Vec<Sample> = Vec::new();
        let stdout = io::stdout();
        loop {
            // drain session events
            while let Ok(ev) = session_events.try_recv() {
                match ev {
                    Event::Stdout(l) => {
                        handle.feed_line(&l);
                        let _ = writeln!(stdout.lock(), "[OUT] {l}");
                    }
                    Event::Stderr(l) => {
                        handle.feed_line(&l);
                        let _ = writeln!(stdout.lock(), "[ERR] {l}");
                    }
                    Event::Exited(code) => {
                        handle.notify_exit();
                        let _ = writeln!(stdout.lock(), "[observer] server exited: {code:?}");
                    }
                    _ => {}
                }
            }
            // drain runner events
            while let Ok(ev) = handle.events.try_recv() {
                print_runner_event(&stdout, ev, &mut collected);
            }

            if handle.is_done() {
                // final drain in case events were emitted after the last poll
                while let Ok(ev) = session_events.try_recv() {
                    if let Event::Exited(code) = ev {
                        let _ = writeln!(stdout.lock(), "[observer] server exited: {code:?}");
                    }
                }
                while let Ok(ev) = handle.events.try_recv() {
                    print_runner_event(&stdout, ev, &mut collected);
                }
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        collected
    };

    if !samples.is_empty() {
        let label = samples
            .last()
            .map(|s| s.scenario.clone())
            .unwrap_or_else(|| "run".into());
        match export::export_run(&config.results_dir, &label, &samples) {
            Ok(art) => {
                eprintln!(
                    "[observer] exported: {} / {}",
                    art.csv.display(),
                    art.json.display()
                );
                eprintln!("[observer] summary:\n{}", export::summary_text(&samples));
            }
            Err(e) => {
                eprintln!("[observer] export failed: {e}");
            }
        }
    } else {
        eprintln!("[observer] no samples collected");
    }

    // If the runner left the server running (no `stop` step), give it a brief
    // window to settle, else kill to avoid orphans.
    if ctrl.is_running() {
        eprintln!("[observer] server still running — sending stop + waiting up to 10s");
        let _ = ctrl.send_cmd("stop");
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < deadline && ctrl.is_running() {
            match session_events.try_recv() {
                Ok(Event::Exited(_)) => break,
                Ok(_) => continue,
                Err(mpsc::TryRecvError::Empty) => thread::sleep(Duration::from_millis(100)),
                Err(mpsc::TryRecvError::Disconnected) => break,
            }
        }
        if ctrl.is_running() {
            eprintln!("[observer] server did not exit within 10s — killing");
        }
    }
    ctrl.kill();
    Ok(())
}

fn print_runner_event(stdout: &io::Stdout, ev: Event, collected: &mut Vec<Sample>) {
    match ev {
        Event::ScenarioStart(n) => {
            let _ = writeln!(stdout.lock(), "[STEP] scenario start: {n}");
        }
        Event::ScenarioDone { name, samples } => {
            let _ = writeln!(stdout.lock(), "[STEP] scenario done: {name} ({samples})");
        }
        Event::StepStart(d) => {
            let _ = writeln!(stdout.lock(), "[STEP] → {d}");
        }
        Event::StepDone(d) => {
            let _ = writeln!(stdout.lock(), "[STEP] ✓ {d}");
        }
        Event::Sample(s) => {
            let text = s
                .metrics
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(stdout.lock(), "[SAMPLE] {text}");
            collected.push(s);
        }
        Event::RunnerInfo(m) => {
            let _ = writeln!(stdout.lock(), "[RUN] {m}");
        }
        Event::RunnerError(m) => {
            let _ = writeln!(stdout.lock(), "[RUN] error: {m}");
        }
        _ => {}
    }
}

fn pick_scenarios(cfg: &Config, names_override: &[String]) -> io::Result<Vec<Scenario>> {
    let loaded = scenario::load_scenarios(&cfg.scenarios_dir);
    let mut by_name = std::collections::BTreeMap::new();
    for r in loaded {
        match r {
            Ok(s) => {
                by_name.insert(s.name.clone(), s);
            }
            Err(e) => eprintln!("[observer] load {}: {}", e.path.display(), e.msg),
        }
    }
    let names: Vec<&String> = if !names_override.is_empty() {
        names_override.iter().collect()
    } else {
        cfg.selected_scenarios.iter().collect()
    };
    let mut out = Vec::new();
    for n in names {
        match by_name.get(n) {
            Some(s) => out.push(s.clone()),
            None => {
                eprintln!("[observer] unknown scenario: {n}");
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!("scenario not found: {n}"),
                ));
            }
        }
    }
    Ok(out)
}
