use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, SystemTime};

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

fn default_scenarios_dir() -> PathBuf {
    PathBuf::from("./scenarios")
}

fn default_results_dir() -> PathBuf {
    PathBuf::from("./results")
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    #[serde(default)]
    pub server_dir: Option<String>,
    #[serde(default)]
    pub server_cmd: Vec<String>,
    #[serde(default = "default_lang")]
    pub lang: Lang,
    #[serde(default = "default_scenarios_dir")]
    pub scenarios_dir: PathBuf,
    #[serde(default)]
    pub selected_scenarios: Vec<String>,
    #[serde(default = "default_results_dir")]
    pub results_dir: PathBuf,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server_dir: None,
            server_cmd: Vec::new(),
            lang: default_lang(),
            scenarios_dir: default_scenarios_dir(),
            selected_scenarios: Vec::new(),
            results_dir: default_results_dir(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Sample {
    pub ts: SystemTime,
    pub scenario: String,
    pub metrics: HashMap<String, f64>,
}

#[derive(Debug, Clone)]
pub enum Event {
    Stdout(String),
    Stderr(String),
    Exited(Option<i32>),
    StepStart(String),
    StepDone(String),
    ScenarioStart(String),
    ScenarioDone {
        name: String,
        samples: usize,
    },
    Sample(Sample),
    RunnerInfo(String),
    RunnerError(String),
}

type Writer = Arc<Mutex<Option<ChildStdin>>>;
type ChildHandle = Arc<Mutex<Option<Child>>>;

pub struct Session {
    writer: Writer,
    child: ChildHandle,
}

impl Session {
    pub fn spawn(config: &Config) -> io::Result<(Self, mpsc::Receiver<Event>)> {
        if config.server_cmd.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "server_cmd is empty",
            ));
        }

        let mut cmd = Command::new(&config.server_cmd[0]);
        cmd.args(&config.server_cmd[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(dir) = &config.server_dir
            && !dir.is_empty()
        {
            cmd.current_dir(dir);
        }
        let mut child = cmd.spawn()?;

        let writer: Writer = Arc::new(Mutex::new(Some(child.stdin.take().expect("stdin"))));
        let stdout = child.stdout.take().expect("stdout");
        let stderr = child.stderr.take().expect("stderr");
        let child_handle: ChildHandle = Arc::new(Mutex::new(Some(child)));

        let (ev_tx, ev_rx) = mpsc::channel();

        {
            let tx = ev_tx.clone();
            thread::spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines().map_while(Result::ok) {
                    if tx.send(Event::Stdout(line)).is_err() {
                        break;
                    }
                }
            });
        }
        {
            let tx = ev_tx.clone();
            thread::spawn(move || {
                let reader = BufReader::new(stderr);
                for line in reader.lines().map_while(Result::ok) {
                    if tx.send(Event::Stderr(line)).is_err() {
                        break;
                    }
                }
            });
        }
        {
            let tx = ev_tx.clone();
            let ch = Arc::clone(&child_handle);
            thread::spawn(move || {
                loop {
                    let status = {
                        let Ok(mut g) = ch.lock() else { return };
                        match g.as_mut() {
                            Some(c) => c.try_wait(),
                            None => return, // killed via Session::kill
                        }
                    };
                    match status {
                        Ok(Some(s)) => {
                            let _ = tx.send(Event::Exited(s.code()));
                            return;
                        }
                        Ok(None) => thread::sleep(Duration::from_millis(200)),
                        Err(_) => return,
                    }
                }
            });
        }

        Ok((
            Session {
                writer,
                child: child_handle,
            },
            ev_rx,
        ))
    }

    pub fn send_cmd(&self, cmd: &str) -> bool {
        write_line(&self.writer, cmd)
    }

    pub fn close_stdin(&self) {
        if let Ok(mut g) = self.writer.lock() {
            g.take();
        }
    }

    /// Kill the server process and wait for it. Safe to call multiple times.
    pub fn kill(&self) {
        self.close_stdin();
        let mut g = match self.child.lock() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        if let Some(mut c) = g.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Centralized session lifecycle. Cheap to clone (shared Arcs).
///
/// Responsibilities:
/// - spawn the server on request (`start`) and kill it on request (`kill`)
/// - forward session events to a single consumer channel (owned by the app)
/// - let runner and TUI query/drive the server through the same handle
///
/// The `starting` flag serializes concurrent `start()` calls: it's set via CAS
/// before `Session::spawn` begins and cleared once the spawn succeeds or fails.
/// Without it, two threads could both pass an early "is Some?" check and spawn
/// two MC processes, orphaning one.
#[derive(Clone)]
pub struct SessionCtrl {
    inner: Arc<Mutex<Option<Arc<Session>>>>,
    starting: Arc<std::sync::atomic::AtomicBool>,
    config: Arc<Mutex<Config>>,
    ev_out: mpsc::Sender<Event>,
}

pub enum StartOutcome {
    Started,
    AlreadyRunning,
}

impl SessionCtrl {
    pub fn new(config: Config) -> (Self, mpsc::Receiver<Event>) {
        let (tx, rx) = mpsc::channel();
        (
            Self {
                inner: Arc::new(Mutex::new(None)),
                starting: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                config: Arc::new(Mutex::new(config)),
                ev_out: tx,
            },
            rx,
        )
    }

    pub fn update_config(&self, cfg: Config) {
        if let Ok(mut g) = self.config.lock() {
            *g = cfg;
        }
    }

    pub fn snapshot_config(&self) -> Config {
        match self.config.lock() {
            Ok(g) => g.clone(),
            Err(p) => p.into_inner().clone(),
        }
    }

    pub fn is_running(&self) -> bool {
        use std::sync::atomic::Ordering;
        if self.starting.load(Ordering::SeqCst) {
            return true;
        }
        self.inner.lock().map(|g| g.is_some()).unwrap_or(false)
    }

    pub fn send_cmd(&self, cmd: &str) -> bool {
        let s = self.session();
        match s {
            Some(s) => s.send_cmd(cmd),
            None => false,
        }
    }

    pub fn close_stdin(&self) {
        if let Some(s) = self.session() {
            s.close_stdin();
        }
    }

    pub fn start(&self) -> Result<StartOutcome, String> {
        use std::sync::atomic::Ordering;
        // Only one thread may be in the spawn-in-progress state at a time.
        if self
            .starting
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Ok(StartOutcome::AlreadyRunning);
        }
        // Guard clears `starting` on every path out of this function, even panics.
        struct StartingGuard<'a>(&'a std::sync::atomic::AtomicBool);
        impl Drop for StartingGuard<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::SeqCst);
            }
        }
        let _guard = StartingGuard(&self.starting);

        {
            let g = self
                .inner
                .lock()
                .map_err(|_| "session lock poisoned".to_string())?;
            if g.is_some() {
                return Ok(StartOutcome::AlreadyRunning);
            }
        }
        let cfg = self.snapshot_config();
        let (session, events) = Session::spawn(&cfg).map_err(|e| e.to_string())?;
        let session = Arc::new(session);
        {
            let mut g = self
                .inner
                .lock()
                .map_err(|_| "session lock poisoned".to_string())?;
            *g = Some(Arc::clone(&session));
        }
        let inner = Arc::clone(&self.inner);
        let tx = self.ev_out.clone();
        let my_session = Arc::clone(&session);
        thread::spawn(move || {
            for ev in events {
                let is_exited = matches!(ev, Event::Exited(_));
                if tx.send(ev).is_err() {
                    break;
                }
                if is_exited {
                    break;
                }
            }
            // Clear `inner` if (and only if) the current occupant is the session
            // this forwarder was responsible for. Prevents a stale forwarder from
            // wiping a freshly-started new session.
            if let Ok(mut g) = inner.lock()
                && g.as_ref()
                    .map(|cur| Arc::ptr_eq(cur, &my_session))
                    .unwrap_or(false)
            {
                g.take();
            }
        });
        Ok(StartOutcome::Started)
    }

    /// Kill the running server (if any). Idempotent.
    pub fn kill(&self) {
        let taken = {
            let mut g = match self.inner.lock() {
                Ok(g) => g,
                Err(p) => p.into_inner(),
            };
            g.take()
        };
        if let Some(s) = taken {
            s.kill();
        }
    }

    fn session(&self) -> Option<Arc<Session>> {
        match self.inner.lock() {
            Ok(g) => g.clone(),
            Err(_) => None,
        }
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

pub fn load_config(path: &str) -> io::Result<Config> {
    let text = fs::read_to_string(path)?;
    let cfg: Config = serde_json::from_str(&text)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("config parse: {e}")))?;
    Ok(cfg)
}

pub fn save_config(path: &str, config: &Config) -> io::Result<()> {
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| io::Error::other(format!("{e}")))?;
    fs::write(path, json)
}
