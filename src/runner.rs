use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use crate::core::{Event, Sample, SessionCtrl, StartOutcome};
use crate::scenario::{Pattern, Scenario, Step};

const GRAB_TIMEOUT: Duration = Duration::from_secs(5);
const STOP_WAIT: Duration = Duration::from_secs(60);

/// Substrings in server output that immediately abort the current scenario.
/// Matched case-sensitively against each fed line. Multiple hits per scenario
/// collapse to one `RunnerError` event (first match wins).
const ABORT_PATTERNS: &[&str] = &[
    "Unknown or incomplete command, see below for error",
];

fn line_triggers_abort(line: &str) -> Option<&'static str> {
    ABORT_PATTERNS
        .iter()
        .copied()
        .find(|p| line.contains(p))
}

pub struct RunnerHandle {
    pub events: mpsc::Receiver<Event>,
    line_tx: mpsc::Sender<String>,
    stop_flag: Arc<AtomicBool>,
    exited_flag: Arc<AtomicBool>,
    error_flag: Arc<AtomicBool>,
    done_flag: Arc<AtomicBool>,
    progress: Arc<Mutex<ProgressState>>,
    event_tx: mpsc::Sender<Event>,
}

impl RunnerHandle {
    pub fn feed_line(&self, line: &str) {
        if let Some(pat) = line_triggers_abort(line)
            && !self.error_flag.swap(true, Ordering::SeqCst)
        {
            let _ = self.event_tx.send(Event::RunnerError(format!(
                "command error detected — aborting scenario ({pat})"
            )));
        }
        let _ = self.line_tx.send(line.to_string());
    }

    /// Signal that the currently-running server has exited.
    /// Aborts in-flight `wait`/`grab`/`sleep`; the next `start` step will clear it.
    pub fn notify_exit(&self) {
        self.exited_flag.store(true, Ordering::SeqCst);
    }

    pub fn request_stop(&self) {
        self.stop_flag.store(true, Ordering::SeqCst);
    }

    pub fn is_done(&self) -> bool {
        self.done_flag.load(Ordering::SeqCst)
    }

    /// Snapshot of progress state; cheap, used by TUI every frame.
    pub fn progress(&self) -> ProgressState {
        match self.progress.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }
}

#[derive(Clone, Default)]
pub struct ProgressState {
    pub scenario: Option<ScenarioProgress>,
    pub step: Option<StepProgress>,
    pub loop_info: Option<LoopProgress>,
}

#[derive(Clone)]
pub struct ScenarioProgress {
    pub name: String,
    pub index: usize, // 1-based
    pub total: usize,
}

#[derive(Clone)]
pub struct StepProgress {
    pub desc: String,
    pub index: usize, // 1-based position in top-level step list
    pub total: usize,
}

#[derive(Clone)]
pub struct LoopProgress {
    pub started_at: Instant,
    pub total: Duration,
    pub iter: u64,
}

pub fn spawn(ctrl: SessionCtrl, scenarios: Vec<Scenario>) -> RunnerHandle {
    let (line_tx, line_rx) = mpsc::channel::<String>();
    let (event_tx, event_rx) = mpsc::channel::<Event>();
    let stop_flag = Arc::new(AtomicBool::new(false));
    let exited_flag = Arc::new(AtomicBool::new(false));
    let error_flag = Arc::new(AtomicBool::new(false));
    let done_flag = Arc::new(AtomicBool::new(false));
    let progress = Arc::new(Mutex::new(ProgressState::default()));

    let ctx = RunCtx {
        line_rx: Arc::new(Mutex::new(line_rx)),
        event_tx: event_tx.clone(),
        ctrl,
        stop_flag: Arc::clone(&stop_flag),
        exited_flag: Arc::clone(&exited_flag),
        error_flag: Arc::clone(&error_flag),
        progress: Arc::clone(&progress),
    };

    {
        let done = Arc::clone(&done_flag);
        let progress = Arc::clone(&progress);
        thread::spawn(move || {
            let scenario_total = scenarios.len();
            for (idx0, sc) in scenarios.iter().enumerate() {
                // only user-requested stop aborts the outer loop; scenarios
                // are free to start/stop the server independently across runs.
                if ctx.stop_flag.load(Ordering::SeqCst) {
                    break;
                }
                // reset per-scenario abort signal so a prior scenario's
                // command error doesn't carry over
                ctx.error_flag.store(false, Ordering::SeqCst);
                update_progress(&ctx.progress, |p| {
                    p.scenario = Some(ScenarioProgress {
                        name: sc.name.clone(),
                        index: idx0 + 1,
                        total: scenario_total,
                    });
                    p.step = None;
                    p.loop_info = None;
                });
                let _ = ctx.event_tx.send(Event::ScenarioStart(sc.name.clone()));
                let mut samples = 0usize;
                if let Err(msg) = run_top_steps(&ctx, sc, &sc.steps, &mut samples) {
                    let _ = ctx.event_tx.send(Event::RunnerError(format!(
                        "scenario {} aborted: {}",
                        sc.name, msg
                    )));
                }
                let _ = ctx.event_tx.send(Event::ScenarioDone {
                    name: sc.name.clone(),
                    samples,
                });
            }
            // clear progress on completion so the panel falls back to "idle"
            update_progress(&progress, |p| *p = ProgressState::default());
            done.store(true, Ordering::SeqCst);
        });
    }

    RunnerHandle {
        events: event_rx,
        line_tx,
        stop_flag,
        exited_flag,
        error_flag,
        done_flag,
        progress,
        event_tx,
    }
}

fn update_progress<F: FnOnce(&mut ProgressState)>(
    slot: &Arc<Mutex<ProgressState>>,
    f: F,
) {
    let mut g = match slot.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    f(&mut g);
}

struct RunCtx {
    line_rx: Arc<Mutex<mpsc::Receiver<String>>>,
    event_tx: mpsc::Sender<Event>,
    ctrl: SessionCtrl,
    stop_flag: Arc<AtomicBool>,
    exited_flag: Arc<AtomicBool>,
    error_flag: Arc<AtomicBool>,
    progress: Arc<Mutex<ProgressState>>,
}

impl RunCtx {
    /// True when the currently-running server has died, the user requested stop,
    /// or a command error pattern was detected in server output.
    /// Used by wait/grab/sleep and loop iteration; the outer scenario loop
    /// checks stop_flag only so a command error aborts *only this scenario*.
    fn aborted(&self) -> bool {
        self.stop_flag.load(Ordering::SeqCst)
            || self.exited_flag.load(Ordering::SeqCst)
            || self.error_flag.load(Ordering::SeqCst)
    }

    fn recv_timeout(&self, timeout: Duration) -> Option<String> {
        let start = Instant::now();
        let step = Duration::from_millis(100);
        loop {
            if self.aborted() {
                return None;
            }
            let remaining = timeout.checked_sub(start.elapsed())?;
            let slice = remaining.min(step);
            let rx = self.line_rx.lock().ok()?;
            match rx.recv_timeout(slice) {
                Ok(l) => return Some(l),
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => return None,
            }
        }
    }
}

fn run_top_steps(
    ctx: &RunCtx,
    sc: &Scenario,
    steps: &[Step],
    samples: &mut usize,
) -> Result<(), String> {
    let total = steps.len();
    for (i, step) in steps.iter().enumerate() {
        if ctx.stop_flag.load(Ordering::SeqCst) {
            return Err("stop requested".into());
        }
        if ctx.error_flag.load(Ordering::SeqCst) {
            return Err("command error — scenario aborted".into());
        }
        update_progress(&ctx.progress, |p| {
            p.step = Some(StepProgress {
                desc: step_desc(step),
                index: i + 1,
                total,
            });
            // leaving a previous Loop step — clear stale loop info
            if !matches!(step, Step::Loop { .. }) {
                p.loop_info = None;
            }
        });
        // wait/grab/sleep/stop themselves respect exited_flag; send fails with
        // a clear message if server is down; start clears the flag.
        run_one(ctx, sc, step, samples)?;
    }
    Ok(())
}

fn run_body_steps(
    ctx: &RunCtx,
    sc: &Scenario,
    steps: &[Step],
    samples: &mut usize,
) -> Result<(), String> {
    for step in steps {
        if ctx.stop_flag.load(Ordering::SeqCst) {
            return Err("stop requested".into());
        }
        if ctx.error_flag.load(Ordering::SeqCst) {
            return Err("command error — scenario aborted".into());
        }
        run_one(ctx, sc, step, samples)?;
    }
    Ok(())
}

fn step_desc(s: &Step) -> String {
    match s {
        Step::Start => "start".into(),
        Step::Wait { pattern, .. } => format!("wait {}", pattern.raw),
        Step::Send(cmd) => format!("send {cmd}"),
        Step::Sleep(d) => format!("sleep {}", fmt_dur(*d)),
        Step::Grab { name, .. } => format!("grab {name}"),
        Step::Loop { total, every, .. } => {
            format!("loop {} every {}", fmt_dur(*total), fmt_dur(*every))
        }
        Step::Stop => "stop".into(),
    }
}

fn run_one(
    ctx: &RunCtx,
    sc: &Scenario,
    step: &Step,
    samples: &mut usize,
) -> Result<(), String> {
    match step {
        Step::Start => {
            let _ = ctx.event_tx.send(Event::StepStart("start".into()));
            // reset before spawning so any leftover exited flag from prior run
            // doesn't immediately abort the new session's wait/grab.
            ctx.exited_flag.store(false, Ordering::SeqCst);
            match ctx.ctrl.start() {
                Ok(StartOutcome::Started) => {
                    let _ = ctx
                        .event_tx
                        .send(Event::RunnerInfo("server started".into()));
                }
                Ok(StartOutcome::AlreadyRunning) => {
                    let _ = ctx
                        .event_tx
                        .send(Event::RunnerInfo("server already running — reusing".into()));
                }
                Err(e) => return Err(format!("start failed: {e}")),
            }
            let _ = ctx.event_tx.send(Event::StepDone("start".into()));
        }
        Step::Wait { timeout, pattern } => {
            let _ = ctx
                .event_tx
                .send(Event::StepStart(format!("wait {}", pattern.raw)));
            wait_for(ctx, pattern, *timeout)?;
            let _ = ctx.event_tx.send(Event::StepDone("wait".into()));
        }
        Step::Send(cmd) => {
            let _ = ctx.event_tx.send(Event::StepStart(format!("send {cmd}")));
            if !ctx.ctrl.send_cmd(cmd) {
                return Err("server stdin unavailable (is it running? add `start` first)".into());
            }
            let _ = ctx.event_tx.send(Event::StepDone("send".into()));
        }
        Step::Sleep(d) => {
            let _ = ctx
                .event_tx
                .send(Event::StepStart(format!("sleep {}", fmt_dur(*d))));
            sleep_interruptible(ctx, *d);
            if ctx.stop_flag.load(Ordering::SeqCst) {
                return Err("stop requested".into());
            }
            let _ = ctx.event_tx.send(Event::StepDone("sleep".into()));
        }
        Step::Grab { name, pattern } => {
            let _ = ctx
                .event_tx
                .send(Event::StepStart(format!("grab {name}")));
            match grab_value(ctx, pattern, GRAB_TIMEOUT) {
                Some(v) => {
                    let mut metrics = HashMap::new();
                    metrics.insert(name.clone(), v);
                    let _ = ctx.event_tx.send(Event::Sample(Sample {
                        ts: SystemTime::now(),
                        scenario: sc.name.clone(),
                        metrics,
                    }));
                    *samples += 1;
                }
                None => {
                    let _ = ctx
                        .event_tx
                        .send(Event::RunnerInfo(format!("grab {name}: no match")));
                }
            }
            let _ = ctx.event_tx.send(Event::StepDone("grab".into()));
        }
        Step::Loop { total, every, body } => {
            let _ = ctx.event_tx.send(Event::StepStart(format!(
                "loop {} every {}",
                fmt_dur(*total),
                fmt_dur(*every)
            )));
            let loop_start = Instant::now();
            update_progress(&ctx.progress, |p| {
                p.loop_info = Some(LoopProgress {
                    started_at: loop_start,
                    total: *total,
                    iter: 0,
                });
            });
            let mut iter = 0u64;
            while loop_start.elapsed() < *total && !ctx.aborted() {
                let iter_start = Instant::now();
                iter += 1;
                update_progress(&ctx.progress, |p| {
                    if let Some(li) = p.loop_info.as_mut() {
                        li.iter = iter;
                    }
                });
                run_body_steps(ctx, sc, body, samples)?;
                let elapsed = iter_start.elapsed();
                if elapsed < *every {
                    sleep_interruptible(ctx, *every - elapsed);
                } else {
                    let _ = ctx.event_tx.send(Event::RunnerInfo(format!(
                        "loop iter {iter} took {}ms (> interval {}ms)",
                        elapsed.as_millis(),
                        every.as_millis()
                    )));
                }
            }
            update_progress(&ctx.progress, |p| {
                p.loop_info = None;
            });
            let _ = ctx
                .event_tx
                .send(Event::StepDone(format!("loop ({iter} iters)")));
        }
        Step::Stop => {
            let _ = ctx.event_tx.send(Event::StepStart("stop".into()));
            let _ = ctx.ctrl.send_cmd("stop");
            let deadline = Instant::now() + STOP_WAIT;
            while !ctx.exited_flag.load(Ordering::SeqCst) && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(100));
                if ctx.stop_flag.load(Ordering::SeqCst) {
                    break;
                }
            }
            if !ctx.exited_flag.load(Ordering::SeqCst) {
                // didn't exit in time — force kill to avoid orphan
                ctx.ctrl.kill();
                ctx.exited_flag.store(true, Ordering::SeqCst);
                let _ = ctx.event_tx.send(Event::RunnerInfo(
                    "server didn't exit within stop window — killed".into(),
                ));
            }
            let _ = ctx.event_tx.send(Event::StepDone("stop".into()));
        }
    }
    Ok(())
}

fn wait_for(ctx: &RunCtx, pattern: &Pattern, timeout: Duration) -> Result<(), String> {
    let start = Instant::now();
    loop {
        let remaining = match timeout.checked_sub(start.elapsed()) {
            Some(r) if !r.is_zero() => r,
            _ => {
                return Err(format!(
                    "wait timeout after {}ms for {:?}",
                    timeout.as_millis(),
                    pattern.raw
                ));
            }
        };
        let Some(line) = ctx.recv_timeout(remaining) else {
            if ctx.aborted() {
                return Err("aborted".into());
            }
            return Err(format!(
                "wait timeout after {}ms for {:?}",
                timeout.as_millis(),
                pattern.raw
            ));
        };
        if pattern.regex.is_match(&line) {
            return Ok(());
        }
    }
}

fn grab_value(ctx: &RunCtx, pattern: &Pattern, timeout: Duration) -> Option<f64> {
    let start = Instant::now();
    loop {
        let remaining = timeout.checked_sub(start.elapsed())?;
        if remaining.is_zero() {
            return None;
        }
        let line = ctx.recv_timeout(remaining)?;
        if let Some(caps) = pattern.regex.captures(&line)
            && let Some(g1) = caps.get(1)
            && let Ok(v) = g1.as_str().parse::<f64>()
        {
            return Some(v);
        }
    }
}

fn sleep_interruptible(ctx: &RunCtx, d: Duration) {
    let end = Instant::now() + d;
    while Instant::now() < end {
        if ctx.stop_flag.load(Ordering::SeqCst) {
            return;
        }
        let slice = (end - Instant::now()).min(Duration::from_millis(100));
        thread::sleep(slice);
    }
}

fn fmt_dur(d: Duration) -> String {
    let ms = d.as_millis();
    if ms >= 60_000 && ms.is_multiple_of(60_000) {
        format!("{}m", ms / 60_000)
    } else if ms >= 1000 && ms.is_multiple_of(1000) {
        format!("{}s", ms / 1000)
    } else {
        format!("{ms}ms")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abort_pattern_matches_vanilla_line() {
        let line = "[00:00:01] [Server thread/INFO]: Unknown or incomplete command, see below for error";
        assert_eq!(
            line_triggers_abort(line),
            Some("Unknown or incomplete command, see below for error")
        );
    }

    #[test]
    fn abort_pattern_ignores_unrelated() {
        assert!(line_triggers_abort("[INFO] Done (3.141s)! For help, type \"help\"").is_none());
        assert!(line_triggers_abort("Average tick time: 12.5 ms").is_none());
    }
}
