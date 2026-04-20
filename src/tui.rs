use std::collections::{HashSet, VecDeque};
use std::io::{self, Stdout, stdout};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event as CEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::core::{self, Config, Event as SEvent, Lang, Sample, SessionCtrl, StartOutcome};
use crate::export;
use crate::runner::{self, RunnerHandle};
use crate::scenario::{self, Scenario, ScenarioLoadError};
use crate::strings::{L10n, PRESETS, render_tpl1, render_tpl2, t};

const LOG_CAP: usize = 5000;
const DEFAULT_CFG: &str = "./observer.json";

type Term = Terminal<CrosstermBackend<Stdout>>;

#[derive(Clone, Copy, PartialEq)]
enum Focus {
    Input,
    Scenarios,
    Log,
}

#[derive(Clone)]
struct LogLine {
    kind: LogKind,
    text: String,
}

#[derive(Clone, Copy, PartialEq)]
enum LogKind {
    Out,
    Err,
    Info,
    Error,
    Step,
    Sample,
    Runner,
}

struct ConfigForm {
    field: usize,
    server_dir: String,
    server_cmd: String,
    scenarios_dir: String,
    results_dir: String,
    cur: [usize; 4],
}

const CONFIG_FORM_FIELDS: usize = 4;

impl ConfigForm {
    fn new(cfg: &Config) -> Self {
        let dir = cfg.server_dir.clone().unwrap_or_default();
        let cmd = cfg.server_cmd.join(" ");
        let sdir = cfg.scenarios_dir.display().to_string();
        let rdir = cfg.results_dir.display().to_string();
        let cur = [dir.len(), cmd.len(), sdir.len(), rdir.len()];
        Self {
            field: 0,
            server_dir: dir,
            server_cmd: cmd,
            scenarios_dir: sdir,
            results_dir: rdir,
            cur,
        }
    }

    fn apply(&self, cfg: &mut Config, lang: Lang) -> Result<(), String> {
        let cmd_parts: Vec<String> = shell_split(&self.server_cmd);
        if cmd_parts.is_empty() {
            return Err(t(lang).err_server_cmd_empty.into());
        }
        cfg.server_dir = if self.server_dir.trim().is_empty() {
            None
        } else {
            Some(self.server_dir.clone())
        };
        cfg.server_cmd = cmd_parts;
        cfg.scenarios_dir = PathBuf::from(
            if self.scenarios_dir.trim().is_empty() {
                "./scenarios"
            } else {
                self.scenarios_dir.as_str()
            },
        );
        cfg.results_dir = PathBuf::from(if self.results_dir.trim().is_empty() {
            "./results"
        } else {
            self.results_dir.as_str()
        });
        Ok(())
    }

    fn field_buf_mut(&mut self) -> (&mut String, &mut usize) {
        let idx = self.field;
        let cur_ref = &mut self.cur[idx];
        let buf = match idx {
            0 => &mut self.server_dir,
            1 => &mut self.server_cmd,
            2 => &mut self.scenarios_dir,
            3 => &mut self.results_dir,
            _ => unreachable!(),
        };
        (buf, cur_ref)
    }
}

fn shell_split(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut escape = false;
    for ch in s.chars() {
        if escape {
            cur.push(ch);
            escape = false;
            continue;
        }
        match ch {
            '\\' if !in_single => escape = true,
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            c if c.is_whitespace() && !in_single && !in_double => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

enum Modal {
    ConfigForm(ConfigForm),
    Error(String),
    Help,
}

struct Wizard {
    page: usize, // 0 welcome, 1 dir, 2 cmd, 3 scenarios
    dir_buf: String,
    dir_cur: usize,
    preset_idx: usize,
    cmd_buf: String,
    cmd_cur: usize,
    cmd_edit_focus: bool, // when page=2, false=presets list, true=cmd input
    scenario_sel: usize,
    scenario_selected: HashSet<String>,
}

impl Wizard {
    fn new(cfg: &Config, selected: &[String]) -> Self {
        let dir = cfg.server_dir.clone().unwrap_or_default();
        let cmd = cfg.server_cmd.join(" ");
        Self {
            page: 0,
            dir_cur: dir.len(),
            dir_buf: dir,
            preset_idx: 0,
            cmd_cur: cmd.len(),
            cmd_buf: cmd,
            cmd_edit_focus: false,
            scenario_sel: 0,
            scenario_selected: selected.iter().cloned().collect(),
        }
    }
}

struct App {
    config: Config,
    config_path: String,
    ctrl: SessionCtrl,
    events: mpsc::Receiver<SEvent>,
    log: VecDeque<LogLine>,
    log_scroll: usize,
    input: String,
    input_cur: usize,
    focus: Focus,
    scenario_sel: usize,
    modal: Option<Modal>,
    wizard: Option<Wizard>,
    status: String,
    status_until: Option<Instant>,
    should_quit: bool,
    scenarios: Vec<Scenario>,
    scenario_load_errors: Vec<ScenarioLoadError>,
    runner: Option<RunnerHandle>,
    samples: Vec<Sample>,
    cur_step: Option<String>,
    cur_scenario: Option<String>,
}

impl App {
    fn tr(&self) -> &'static L10n {
        t(self.config.lang)
    }

    fn new(config_path: Option<&str>) -> io::Result<Self> {
        let path = config_path.unwrap_or(DEFAULT_CFG).to_string();
        let (config, config_existed) = match core::load_config(&path) {
            Ok(c) => (c, true),
            Err(_) => (Config::default(), false),
        };
        let (ctrl, events) = SessionCtrl::new(config.clone());
        let mut app = Self {
            config,
            config_path: path,
            ctrl,
            events,
            log: VecDeque::new(),
            log_scroll: 0,
            input: String::new(),
            input_cur: 0,
            focus: Focus::Input,
            scenario_sel: 0,
            modal: None,
            wizard: None,
            status: String::new(),
            status_until: None,
            should_quit: false,
            scenarios: Vec::new(),
            scenario_load_errors: Vec::new(),
            runner: None,
            samples: Vec::new(),
            cur_step: None,
            cur_scenario: None,
        };
        let need_wizard = !config_existed
            || app.config.server_dir.as_deref().unwrap_or("").is_empty()
            || app.config.server_cmd.is_empty();
        app.reload_scenarios();
        if need_wizard {
            let selected = app.config.selected_scenarios.clone();
            app.wizard = Some(Wizard::new(&app.config, &selected));
        } else {
            let msg = render_tpl1(app.tr().tpl_loaded_config, &app.config_path);
            app.info(msg);
        }
        Ok(app)
    }

    fn reload_scenarios(&mut self) {
        let results = scenario::load_scenarios(&self.config.scenarios_dir);
        self.scenarios.clear();
        self.scenario_load_errors.clear();
        let mut errors_to_log: Vec<String> = Vec::new();
        for r in results {
            match r {
                Ok(s) => self.scenarios.push(s),
                Err(e) => {
                    errors_to_log.push(render_tpl2(
                        self.tr().tpl_scenario_load_failed,
                        &e.path.display().to_string(),
                        &e.msg,
                    ));
                    self.scenario_load_errors.push(e);
                }
            }
        }
        if self.scenario_sel >= self.scenarios.len() {
            self.scenario_sel = self.scenarios.len().saturating_sub(1);
        }
        if !self.scenarios.is_empty() {
            let msg = render_tpl2(
                self.tr().tpl_scenarios_loaded,
                &self.config.scenarios_dir.display().to_string(),
                &self.scenarios.len().to_string(),
            );
            self.push_log(LogKind::Info, msg);
        }
        for m in errors_to_log {
            self.push_log(LogKind::Error, m);
        }
    }

    fn push_log(&mut self, kind: LogKind, text: String) {
        if self.log.len() >= LOG_CAP {
            self.log.pop_front();
        }
        self.log.push_back(LogLine { kind, text });
    }

    fn info(&mut self, s: String) {
        self.status(s.clone());
        self.push_log(LogKind::Info, s);
    }

    fn error(&mut self, s: String) {
        self.status(format!("ERROR: {s}"));
        self.push_log(LogKind::Error, s);
    }

    fn status(&mut self, s: String) {
        self.status = s;
        self.status_until = Some(Instant::now() + Duration::from_secs(4));
    }

    fn drain_session(&mut self) {
        let mut batch = Vec::new();
        while let Ok(ev) = self.events.try_recv() {
            batch.push(ev);
        }
        for ev in batch {
            match ev {
                SEvent::Stdout(l) => {
                    if let Some(r) = &self.runner {
                        r.feed_line(&l);
                    }
                    self.push_log(LogKind::Out, l);
                }
                SEvent::Stderr(l) => {
                    if let Some(r) = &self.runner {
                        r.feed_line(&l);
                    }
                    self.push_log(LogKind::Err, l);
                }
                SEvent::Exited(code) => {
                    if let Some(r) = &self.runner {
                        r.notify_exit();
                    }
                    let msg = render_tpl1(
                        self.tr().tpl_server_exited,
                        &code.unwrap_or(-1).to_string(),
                    );
                    self.info(msg);
                }
                _ => {} // SessionCtrl only forwards server events; runner has its own channel
            }
        }
    }

    fn drain_runner(&mut self) {
        let events: Vec<SEvent> = match &self.runner {
            Some(r) => {
                let mut out = Vec::new();
                while let Ok(ev) = r.events.try_recv() {
                    out.push(ev);
                }
                out
            }
            None => return,
        };
        for ev in events {
            match ev {
                SEvent::ScenarioStart(name) => {
                    self.cur_scenario = Some(name.clone());
                    self.push_log(
                        LogKind::Step,
                        render_tpl1(self.tr().tpl_scenario_start, &name),
                    );
                }
                SEvent::ScenarioDone { name, samples } => {
                    self.cur_scenario = None;
                    self.cur_step = None;
                    let msg = render_tpl2(
                        self.tr().tpl_scenario_done,
                        &name,
                        &samples.to_string(),
                    );
                    self.push_log(LogKind::Step, msg);
                }
                SEvent::StepStart(desc) => {
                    self.cur_step = Some(desc.clone());
                    self.push_log(LogKind::Step, render_tpl1(self.tr().tpl_step_start, &desc));
                }
                SEvent::StepDone(desc) => {
                    self.push_log(LogKind::Step, render_tpl1(self.tr().tpl_step_done, &desc));
                }
                SEvent::Sample(sample) => {
                    let text = sample
                        .metrics
                        .iter()
                        .map(|(k, v)| format!("{k}={v}"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    self.push_log(LogKind::Sample, render_tpl1(self.tr().tpl_sample, &text));
                    self.samples.push(sample);
                }
                SEvent::RunnerInfo(msg) => {
                    self.push_log(LogKind::Runner, render_tpl1(self.tr().tpl_runner_info, &msg));
                }
                SEvent::RunnerError(msg) => {
                    self.push_log(LogKind::Runner, render_tpl1(self.tr().tpl_runner_error, &msg));
                }
                _ => {}
            }
        }
        if let Some(r) = &self.runner
            && r.is_done()
        {
            self.runner = None;
            self.cur_scenario = None;
            self.cur_step = None;
        }
    }

    fn start_server(&mut self) {
        if self.ctrl.is_running() {
            self.status(self.tr().server_already_running.into());
            return;
        }
        if self.config.server_cmd.is_empty() {
            self.error(self.tr().server_cmd_empty_hint.into());
            return;
        }
        match self.ctrl.start() {
            Ok(StartOutcome::Started) => {
                let cmd = self.config.server_cmd.join(" ");
                let dir = self.config.server_dir.clone().unwrap_or_else(|| ".".into());
                let msg = render_tpl2(self.tr().tpl_started_at, &cmd, &dir);
                self.info(msg);
            }
            Ok(StartOutcome::AlreadyRunning) => {
                self.status(self.tr().server_already_running.into());
            }
            Err(e) => {
                let msg = render_tpl1(self.tr().tpl_spawn_failed, &e);
                self.error(msg);
            }
        }
    }

    fn stop_server(&mut self) {
        if !self.ctrl.is_running() {
            self.status(self.tr().no_server_running.into());
            return;
        }
        if self.ctrl.send_cmd("stop") {
            self.info(self.tr().sent_stop.into());
        } else {
            self.error(self.tr().stdin_unavailable.into());
        }
    }

    fn start_run(&mut self) {
        if self.runner.is_some() {
            self.status(self.tr().run_already_active.into());
            return;
        }
        let picks: Vec<Scenario> = self
            .scenarios
            .iter()
            .filter(|s| self.config.selected_scenarios.iter().any(|n| n == &s.name))
            .cloned()
            .collect();
        if picks.is_empty() {
            self.status(self.tr().run_no_selection.into());
            return;
        }
        let n = picks.len();
        let handle = runner::spawn(self.ctrl.clone(), picks);
        self.runner = Some(handle);
        let msg = render_tpl1(self.tr().tpl_run_started, &n.to_string());
        self.info(msg);
    }

    fn export_samples(&mut self) {
        if self.samples.is_empty() {
            self.status(self.tr().no_samples_to_export.into());
            return;
        }
        let label = self
            .samples
            .last()
            .map(|s| s.scenario.clone())
            .unwrap_or_else(|| "run".into());
        match export::export_run(&self.config.results_dir, &label, &self.samples) {
            Ok(art) => {
                let msg = render_tpl2(
                    self.tr().tpl_export_ok,
                    &art.csv.display().to_string(),
                    &art.json.display().to_string(),
                );
                self.info(msg);
                let summary = export::summary_text(&self.samples);
                self.push_log(LogKind::Sample, format!("{}:", self.tr().summary_title));
                for line in summary.lines() {
                    self.push_log(LogKind::Sample, line.to_string());
                }
            }
            Err(e) => {
                let msg = render_tpl1(self.tr().tpl_export_failed, &e.to_string());
                self.error(msg);
            }
        }
    }

    fn toggle_selected_scenario(&mut self) {
        if self.scenario_sel >= self.scenarios.len() {
            return;
        }
        let name = self.scenarios[self.scenario_sel].name.clone();
        let pos = self.config.selected_scenarios.iter().position(|n| n == &name);
        match pos {
            Some(i) => {
                self.config.selected_scenarios.remove(i);
            }
            None => self.config.selected_scenarios.push(name),
        }
        self.autosave();
    }

    fn send_input(&mut self) {
        let line = std::mem::take(&mut self.input);
        self.input_cur = 0;
        if line.is_empty() {
            return;
        }
        if line == ":quit" {
            if self.ctrl.is_running() {
                self.ctrl.close_stdin();
                self.info(self.tr().closed_server_stdin.into());
            }
            return;
        }
        if !self.ctrl.is_running() {
            self.error(self.tr().no_server_hint.into());
            return;
        }
        if !self.ctrl.send_cmd(&line) {
            self.error(self.tr().stdin_unavailable.into());
        }
    }

    fn autosave(&mut self) {
        self.ctrl.update_config(self.config.clone());
        if let Err(e) = core::save_config(&self.config_path, &self.config) {
            let msg = render_tpl1(self.tr().tpl_save_failed, &e.to_string());
            self.error(msg);
        }
    }

    fn toggle_lang(&mut self) {
        self.config.lang = match self.config.lang {
            Lang::En => Lang::Zh,
            Lang::Zh => Lang::En,
        };
        self.autosave();
    }

    fn run(&mut self, terminal: &mut Term) -> io::Result<()> {
        let tick = Duration::from_millis(80);
        while !self.should_quit {
            self.drain_session();
            self.drain_runner();
            terminal.draw(|f| draw(f, self))?;
            if event::poll(tick)? {
                match event::read()? {
                    CEvent::Key(k) if k.kind == KeyEventKind::Press => self.handle_key(k),
                    _ => {}
                }
            }
            if let Some(deadline) = self.status_until
                && Instant::now() >= deadline
            {
                self.status.clear();
                self.status_until = None;
            }
        }
        if let Some(r) = &self.runner {
            r.request_stop();
        }
        if self.ctrl.is_running() {
            let _ = self.ctrl.send_cmd("stop");
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline && self.ctrl.is_running() {
                if matches!(self.events.try_recv(), Ok(SEvent::Exited(_))) {
                    break;
                }
                thread::sleep(Duration::from_millis(100));
            }
            self.ctrl.kill();
        }
        Ok(())
    }

    fn handle_key(&mut self, k: KeyEvent) {
        if self.wizard.is_some() {
            self.handle_wizard_key(k);
            return;
        }
        if self.modal.is_some() {
            self.handle_modal_key(k);
            return;
        }
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && k.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }

        if self.focus == Focus::Input {
            match k.code {
                KeyCode::Esc => self.focus = Focus::Log,
                KeyCode::Enter => self.send_input(),
                KeyCode::Tab => self.focus = Focus::Scenarios,
                KeyCode::Left => {
                    if self.input_cur > 0 {
                        self.input_cur -= prev_char_len(&self.input, self.input_cur);
                    }
                }
                KeyCode::Right => {
                    if self.input_cur < self.input.len() {
                        self.input_cur += next_char_len(&self.input, self.input_cur);
                    }
                }
                KeyCode::Home => self.input_cur = 0,
                KeyCode::End => self.input_cur = self.input.len(),
                KeyCode::Backspace => {
                    if self.input_cur > 0 {
                        let n = prev_char_len(&self.input, self.input_cur);
                        let new_cur = self.input_cur - n;
                        self.input.drain(new_cur..self.input_cur);
                        self.input_cur = new_cur;
                    }
                }
                KeyCode::Delete => {
                    if self.input_cur < self.input.len() {
                        let n = next_char_len(&self.input, self.input_cur);
                        self.input.drain(self.input_cur..self.input_cur + n);
                    }
                }
                KeyCode::Char(c) => {
                    let mut buf = [0u8; 4];
                    let s = c.encode_utf8(&mut buf);
                    self.input.insert_str(self.input_cur, s);
                    self.input_cur += s.len();
                }
                _ => {}
            }
            return;
        }

        match k.code {
            KeyCode::Tab => {
                self.focus = match self.focus {
                    Focus::Input => Focus::Scenarios,
                    Focus::Scenarios => Focus::Log,
                    Focus::Log => Focus::Input,
                };
            }
            KeyCode::Char('i') | KeyCode::Char('/') => self.focus = Focus::Input,
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('s') => self.start_server(),
            KeyCode::Char('S') => self.stop_server(),
            KeyCode::Char('r') => self.start_run(),
            KeyCode::Char('x') => self.export_samples(),
            KeyCode::Char(' ') => {
                if self.focus == Focus::Scenarios {
                    self.toggle_selected_scenario();
                }
            }
            KeyCode::Char('j') => self.nav_down(),
            KeyCode::Char('k') => self.nav_up(),
            KeyCode::Char('c') => {
                self.modal = Some(Modal::ConfigForm(ConfigForm::new(&self.config)));
            }
            KeyCode::Char('L') => self.toggle_lang(),
            KeyCode::Char('?') => self.modal = Some(Modal::Help),
            KeyCode::Up => self.nav_up(),
            KeyCode::Down => self.nav_down(),
            KeyCode::PageUp => {
                if self.focus == Focus::Log {
                    self.log_scroll = self.log_scroll.saturating_add(10);
                }
            }
            KeyCode::PageDown => {
                if self.focus == Focus::Log {
                    self.log_scroll = self.log_scroll.saturating_sub(10);
                }
            }
            KeyCode::Home => {
                if self.focus == Focus::Log {
                    self.log_scroll = usize::MAX / 2;
                }
            }
            KeyCode::End => {
                if self.focus == Focus::Log {
                    self.log_scroll = 0;
                }
            }
            _ => {}
        }
    }

    fn nav_up(&mut self) {
        match self.focus {
            Focus::Scenarios => {
                if self.scenario_sel > 0 {
                    self.scenario_sel -= 1;
                }
            }
            Focus::Log => self.log_scroll = self.log_scroll.saturating_add(1),
            _ => {}
        }
    }

    fn nav_down(&mut self) {
        match self.focus {
            Focus::Scenarios => {
                if self.scenario_sel + 1 < self.scenarios.len() {
                    self.scenario_sel += 1;
                }
            }
            Focus::Log => self.log_scroll = self.log_scroll.saturating_sub(1),
            _ => {}
        }
    }

    fn handle_modal_key(&mut self, k: KeyEvent) {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        match self.modal.as_mut() {
            Some(Modal::Error(_)) | Some(Modal::Help) => {
                if matches!(k.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q')) {
                    self.modal = None;
                }
            }
            Some(Modal::ConfigForm(f)) => {
                if k.code == KeyCode::Esc {
                    self.modal = None;
                    return;
                }
                if ctrl && k.code == KeyCode::Char('s') {
                    let mut cfg = self.config.clone();
                    let lang = self.config.lang;
                    match f.apply(&mut cfg, lang) {
                        Ok(()) => {
                            let scenarios_changed = cfg.scenarios_dir != self.config.scenarios_dir;
                            self.config = cfg;
                            self.modal = None;
                            self.info(self.tr().config_updated_hint.into());
                            self.autosave();
                            if scenarios_changed {
                                self.reload_scenarios();
                            }
                        }
                        Err(e) => self.modal = Some(Modal::Error(e)),
                    }
                    return;
                }
                if k.code == KeyCode::Tab {
                    f.field = (f.field + 1) % CONFIG_FORM_FIELDS;
                    return;
                }
                if k.code == KeyCode::BackTab {
                    f.field = (f.field + CONFIG_FORM_FIELDS - 1) % CONFIG_FORM_FIELDS;
                    return;
                }
                let (buf, cur) = f.field_buf_mut();
                edit_single(buf, cur, k);
            }
            None => {}
        }
    }

    fn handle_wizard_key(&mut self, k: KeyEvent) {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && k.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }
        let Some(w) = self.wizard.as_mut() else {
            return;
        };
        if k.code == KeyCode::Esc {
            // skip wizard, use whatever defaults are in place
            self.wizard = None;
            let msg = render_tpl1(self.tr().tpl_loaded_config, &self.config_path);
            self.info(msg);
            return;
        }
        match w.page {
            0 => {
                if k.code == KeyCode::Enter {
                    w.page = 1;
                }
            }
            1 => match k.code {
                KeyCode::Enter => {
                    let dir = w.dir_buf.trim();
                    if !dir.is_empty() && !Path::new(dir).is_dir() {
                        self.modal =
                            Some(Modal::Error(self.tr().err_server_dir_missing.into()));
                        return;
                    }
                    w.page = 2;
                }
                _ => edit_single(&mut w.dir_buf, &mut w.dir_cur, k),
            },
            2 => {
                if k.code == KeyCode::Enter {
                    if shell_split(&w.cmd_buf).is_empty() {
                        self.modal =
                            Some(Modal::Error(self.tr().err_server_cmd_empty.into()));
                        return;
                    }
                    w.page = 3;
                    return;
                }
                if k.code == KeyCode::Tab {
                    w.cmd_edit_focus = !w.cmd_edit_focus;
                    return;
                }
                if !w.cmd_edit_focus {
                    match k.code {
                        KeyCode::Up => {
                            if w.preset_idx > 0 {
                                w.preset_idx -= 1;
                                w.cmd_buf = PRESETS[w.preset_idx].cmd.to_string();
                                w.cmd_cur = w.cmd_buf.len();
                            }
                        }
                        KeyCode::Down => {
                            if w.preset_idx + 1 < PRESETS.len() {
                                w.preset_idx += 1;
                                w.cmd_buf = PRESETS[w.preset_idx].cmd.to_string();
                                w.cmd_cur = w.cmd_buf.len();
                            }
                        }
                        KeyCode::Char(c) => {
                            // typing switches to cmd edit
                            w.cmd_edit_focus = true;
                            let mut buf = [0u8; 4];
                            let s = c.encode_utf8(&mut buf);
                            w.cmd_buf.insert_str(w.cmd_cur, s);
                            w.cmd_cur += s.len();
                        }
                        _ => {}
                    }
                } else {
                    edit_single(&mut w.cmd_buf, &mut w.cmd_cur, k);
                }
            }
            3 => match k.code {
                KeyCode::Up => {
                    if w.scenario_sel > 0 {
                        w.scenario_sel -= 1;
                    }
                }
                KeyCode::Down => {
                    if w.scenario_sel + 1 < self.scenarios.len() {
                        w.scenario_sel += 1;
                    }
                }
                KeyCode::Char(' ') => {
                    if let Some(sc) = self.scenarios.get(w.scenario_sel)
                        && !w.scenario_selected.remove(&sc.name)
                    {
                        w.scenario_selected.insert(sc.name.clone());
                    }
                }
                KeyCode::Enter => {
                    self.finish_wizard();
                }
                _ => {}
            },
            _ => {}
        }
    }

    fn finish_wizard(&mut self) {
        let Some(w) = self.wizard.take() else { return };
        self.config.server_dir = if w.dir_buf.trim().is_empty() {
            None
        } else {
            Some(w.dir_buf.trim().to_string())
        };
        self.config.server_cmd = shell_split(&w.cmd_buf);
        self.config.selected_scenarios = w.scenario_selected.into_iter().collect();
        self.config.selected_scenarios.sort();
        self.autosave();
        let msg = render_tpl1(self.tr().tpl_loaded_config, &self.config_path);
        self.info(msg);
    }
}

fn prev_char_len(s: &str, idx: usize) -> usize {
    s[..idx].chars().next_back().map(|c| c.len_utf8()).unwrap_or(0)
}
fn next_char_len(s: &str, idx: usize) -> usize {
    s[idx..].chars().next().map(|c| c.len_utf8()).unwrap_or(0)
}

fn edit_single(buf: &mut String, cur: &mut usize, k: KeyEvent) {
    match k.code {
        KeyCode::Left => {
            if *cur > 0 {
                *cur -= prev_char_len(buf, *cur);
            }
        }
        KeyCode::Right => {
            if *cur < buf.len() {
                *cur += next_char_len(buf, *cur);
            }
        }
        KeyCode::Home => *cur = 0,
        KeyCode::End => *cur = buf.len(),
        KeyCode::Backspace => {
            if *cur > 0 {
                let n = prev_char_len(buf, *cur);
                let nc = *cur - n;
                buf.drain(nc..*cur);
                *cur = nc;
            }
        }
        KeyCode::Delete => {
            if *cur < buf.len() {
                let n = next_char_len(buf, *cur);
                buf.drain(*cur..*cur + n);
            }
        }
        KeyCode::Char(c) => {
            let mut b = [0u8; 4];
            let s = c.encode_utf8(&mut b);
            buf.insert_str(*cur, s);
            *cur += s.len();
        }
        _ => {}
    }
}

fn setup_terminal() -> io::Result<Term> {
    enable_raw_mode()?;
    let mut out = stdout();
    execute!(out, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(out);
    Terminal::new(backend)
}

fn restore_terminal(terminal: &mut Term) -> io::Result<()> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

pub fn run(config_path: Option<&str>) -> io::Result<()> {
    let mut app = App::new(config_path)?;
    let mut terminal = setup_terminal()?;
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| app.run(&mut terminal)));
    let _ = restore_terminal(&mut terminal);
    match res {
        Ok(r) => r,
        Err(_) => Err(io::Error::other("TUI panicked")),
    }
}

// ---------- rendering ----------

fn draw(f: &mut ratatui::Frame, app: &mut App) {
    let area = f.area();
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title
            Constraint::Min(5),    // body
            Constraint::Length(3), // input
            Constraint::Length(1), // hints
        ])
        .split(area);

    draw_title(f, root[0], app);
    draw_body(f, root[1], app);
    draw_input(f, root[2], app);
    draw_hints(f, root[3], app);

    if let Some(w) = &app.wizard {
        draw_wizard(f, area, w, app);
    } else if let Some(m) = &app.modal {
        let l = app.tr();
        match m {
            Modal::Help => draw_help(f, area, l),
            Modal::Error(msg) => draw_error(f, area, msg, l),
            Modal::ConfigForm(form) => draw_config_form(f, area, form, l),
        }
    }
}

fn draw_title(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let l = app.tr();
    let running = if app.ctrl.is_running() {
        Span::styled(l.running.to_string(), Style::default().fg(Color::Green))
    } else {
        Span::styled(l.stopped.to_string(), Style::default().fg(Color::DarkGray))
    };
    let status = if app.status.is_empty() {
        Span::raw(format!("{}{}", l.config_prefix, app.config_path))
    } else {
        Span::styled(format!("  {}", app.status), Style::default().fg(Color::Yellow))
    };
    let line = Line::from(vec![
        Span::styled(
            l.banner.to_string(),
            Style::default().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        running,
        status,
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn draw_body(f: &mut ratatui::Frame, area: Rect, app: &mut App) {
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(area);

    draw_left(f, cols[0], app);
    draw_log(f, cols[1], app);
}

fn draw_left(f: &mut ratatui::Frame, area: Rect, app: &mut App) {
    let l = app.tr();
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(5),
            Constraint::Min(3),
            Constraint::Length(8),
        ])
        .split(area);

    draw_server_panel(f, rows[0], app);
    draw_scenarios_panel(f, rows[1], app);
    draw_progress_panel(f, rows[2], app);

    let _ = l; // silence if unused
}

fn draw_server_panel(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let l = app.tr();
    let dir = app
        .config
        .server_dir
        .clone()
        .unwrap_or_else(|| l.none_value.to_string());
    let cmd = if app.config.server_cmd.is_empty() {
        l.none_value.to_string()
    } else {
        app.config.server_cmd.join(" ")
    };
    let lines = vec![
        Line::from(vec![
            Span::styled(l.dir_prefix.to_string(), Style::default().fg(Color::DarkGray)),
            Span::raw(dir),
        ]),
        Line::from(vec![
            Span::styled(l.cmd_prefix.to_string(), Style::default().fg(Color::DarkGray)),
            Span::raw(cmd),
        ]),
        Line::from(Span::styled(
            l.press_c_hint.to_string(),
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
        )),
    ];
    f.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(l.server_panel.to_string()),
        ),
        area,
    );
}

fn draw_scenarios_panel(f: &mut ratatui::Frame, area: Rect, app: &mut App) {
    let l = app.tr();
    let focused = app.focus == Focus::Scenarios;
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let title = render_tpl1(l.scenarios_title, &app.scenarios.len().to_string());
    if app.scenarios.is_empty() && app.scenario_load_errors.is_empty() {
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                l.no_scenarios_loaded.to_string(),
                Style::default().fg(Color::DarkGray),
            )))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(border_style)
                    .title(title),
            ),
            area,
        );
        return;
    }
    let selected: Vec<String> = app.config.selected_scenarios.clone();
    let items: Vec<ListItem> = app
        .scenarios
        .iter()
        .map(|s| {
            let is_sel = selected.iter().any(|n| n == &s.name);
            let mark = if is_sel { l.selected_tag } else { "[ ]" };
            let color = if is_sel { Color::Green } else { Color::DarkGray };
            let line = Line::from(vec![
                Span::styled(format!("{mark} "), Style::default().fg(color)),
                Span::styled(s.name.clone(), Style::default().fg(Color::Cyan)),
                Span::styled(
                    format!("  ({} steps)", s.steps.len()),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(title),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");
    let mut state = ListState::default();
    if !app.scenarios.is_empty() {
        state.select(Some(app.scenario_sel.min(app.scenarios.len() - 1)));
    }
    f.render_stateful_widget(list, area, &mut state);
}

fn draw_progress_panel(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let l = app.tr();
    let progress = app
        .runner
        .as_ref()
        .map(|r| r.progress())
        .unwrap_or_default();

    // inner width minus 2 border chars, reserve 10 chars for trailing "  100%"
    let bar_w = (area.width as usize).saturating_sub(2 + 10).max(8);

    let mut body: Vec<Line<'static>> = Vec::new();
    match &progress.scenario {
        Some(sp) => {
            body.push(Line::from(vec![
                Span::styled("▶ ", Style::default().fg(Color::Green)),
                Span::styled(sp.name.clone(), Style::default().fg(Color::Cyan)),
                Span::styled(
                    format!(" ({}/{})", sp.index, sp.total),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
            if let Some(st) = &progress.step {
                body.push(Line::from(vec![
                    Span::styled(
                        format!("step {}/{}: ", st.index, st.total),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(st.desc.clone(), Style::default().fg(Color::Yellow)),
                ]));
                let frac = if st.total == 0 {
                    0.0
                } else {
                    st.index as f32 / st.total as f32
                };
                body.push(bar_line(frac, bar_w, Color::Cyan));
            }
            if let Some(li) = &progress.loop_info {
                let elapsed = Instant::now().saturating_duration_since(li.started_at);
                let elapsed = elapsed.min(li.total);
                let frac = if li.total.as_millis() == 0 {
                    1.0
                } else {
                    elapsed.as_millis() as f32 / li.total.as_millis() as f32
                };
                body.push(Line::from(vec![
                    Span::styled(
                        format!("loop iter {} · ", li.iter),
                        Style::default().fg(Color::DarkGray),
                    ),
                    Span::styled(
                        format!("{}/{}", fmt_dur_brief(elapsed), fmt_dur_brief(li.total)),
                        Style::default().fg(Color::Magenta),
                    ),
                ]));
                body.push(bar_line(frac, bar_w, Color::Magenta));
            }
        }
        None => {
            body.push(Line::from(Span::styled(
                l.progress_idle.to_string(),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::ITALIC),
            )));
        }
    }
    f.render_widget(
        Paragraph::new(body).block(
            Block::default()
                .borders(Borders::ALL)
                .title(l.progress_title.to_string()),
        ),
        area,
    );
}

fn bar_line(frac: f32, width: usize, color: Color) -> Line<'static> {
    let frac = frac.clamp(0.0, 1.0);
    let filled = ((frac * width as f32).round() as usize).min(width);
    let empty = width - filled;
    let pct = (frac * 100.0).round() as u32;
    Line::from(vec![
        Span::styled("█".repeat(filled), Style::default().fg(color)),
        Span::styled(
            "░".repeat(empty),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            format!(" {pct:>3}%"),
            Style::default().fg(Color::DarkGray),
        ),
    ])
}

fn fmt_dur_brief(d: Duration) -> String {
    let ms = d.as_millis();
    if ms >= 60_000 {
        let s = ms / 1000;
        format!("{}m{:02}s", s / 60, s % 60)
    } else if ms >= 1000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else {
        format!("{ms}ms")
    }
}

fn draw_log(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let focused = app.focus == Focus::Log;
    let border_style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let inner_w = area.width.saturating_sub(2) as usize;
    let inner_h = area.height.saturating_sub(2) as usize;

    let mut all_rows: Vec<Line<'static>> = Vec::with_capacity(app.log.len());
    for l in &app.log {
        all_rows.extend(wrap_log_line(l, inner_w));
    }
    let total = all_rows.len();
    let scroll = app.log_scroll.min(total.saturating_sub(inner_h));
    let end = total.saturating_sub(scroll);
    let start = end.saturating_sub(inner_h);
    let visible: Vec<Line<'static>> = all_rows.drain(start..end).collect();

    let l = app.tr();
    let title = if scroll == 0 {
        l.output_following.to_string()
    } else {
        render_tpl1(l.output_scroll, &scroll.to_string())
    };
    f.render_widget(
        Paragraph::new(visible).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(title),
        ),
        area,
    );
}

fn wrap_log_line(l: &LogLine, width: usize) -> Vec<Line<'static>> {
    let (tag, color) = match l.kind {
        LogKind::Out => ("[OUT] ", Color::White),
        LogKind::Err => ("[ERR] ", Color::Red),
        LogKind::Info => ("[observer] ", Color::Cyan),
        LogKind::Error => ("[observer ERR] ", Color::Red),
        LogKind::Step => ("[STEP] ", Color::Yellow),
        LogKind::Sample => ("[SAMPLE] ", Color::Green),
        LogKind::Runner => ("[RUN] ", Color::Magenta),
    };
    let tag_w = tag.chars().count();
    if width <= tag_w + 1 {
        return vec![Line::from(vec![
            Span::styled(tag.to_string(), Style::default().fg(color)),
            Span::raw(l.text.clone()),
        ])];
    }
    let content_w = width - tag_w;
    let chars: Vec<char> = l.text.chars().collect();
    if chars.is_empty() {
        return vec![Line::from(vec![Span::styled(
            tag.to_string(),
            Style::default().fg(color),
        )])];
    }
    let pad: String = " ".repeat(tag_w);
    let mut out = Vec::new();
    let mut i = 0;
    let mut first = true;
    while i < chars.len() {
        let j = (i + content_w).min(chars.len());
        let slice: String = chars[i..j].iter().collect();
        if first {
            out.push(Line::from(vec![
                Span::styled(tag.to_string(), Style::default().fg(color)),
                Span::raw(slice),
            ]));
            first = false;
        } else {
            out.push(Line::from(vec![Span::raw(pad.clone()), Span::raw(slice)]));
        }
        i = j;
    }
    out
}

fn draw_input(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let focused = app.focus == Focus::Input;
    let style = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let l = app.tr();
    let title = if app.ctrl.is_running() {
        l.send_active
    } else {
        l.send_inactive
    };
    let p = Paragraph::new(app.input.clone()).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(style)
            .title(title.to_string()),
    );
    f.render_widget(p, area);
    if focused {
        let col = area.x + 1 + app.input[..app.input_cur.min(app.input.len())].chars().count() as u16;
        f.set_cursor_position((col.min(area.x + area.width - 2), area.y + 1));
    }
}

fn draw_hints(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let l = app.tr();
    let line = Line::from(vec![
        hint("s", l.hint_start),
        hint("S", l.hint_stop),
        hint("c", l.hint_config),
        hint("r", l.hint_run),
        hint("␣", l.hint_toggle),
        hint("x", l.hint_export),
        hint("i", l.hint_input),
        hint("Tab", l.hint_focus),
        hint("L", l.hint_lang),
        hint("?", l.hint_help),
        hint("q", l.hint_quit),
    ]);
    f.render_widget(Paragraph::new(line), area);
}

fn hint(k: &str, label: &str) -> Span<'static> {
    Span::styled(
        format!("[{k}]{label}"),
        Style::default().fg(Color::DarkGray),
    )
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 2,
        width: w,
        height: h,
    }
}

fn draw_help(f: &mut ratatui::Frame, area: Rect, l: &L10n) {
    let h = (l.help_body.len() as u16 + 4).min(area.height);
    let r = centered(area, 64, h);
    f.render_widget(Clear, r);
    let mut text: Vec<Line<'static>> =
        l.help_body.iter().map(|s| Line::from(s.to_string())).collect();
    text.push(Line::from(""));
    text.push(Line::from(l.press_any_to_close.to_string()));
    f.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .borders(Borders::ALL)
                .title(l.help_title.to_string()),
        ),
        r,
    );
}

fn draw_error(f: &mut ratatui::Frame, area: Rect, msg: &str, l: &L10n) {
    let r = centered(area, 60, 7);
    f.render_widget(Clear, r);
    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                msg.to_string(),
                Style::default().fg(Color::Red),
            )),
            Line::from(""),
            Line::from(l.press_esc_or_enter.to_string()),
        ])
        .wrap(Wrap { trim: false })
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(l.error_title.to_string()),
        ),
        r,
    );
}

fn draw_config_form(f: &mut ratatui::Frame, area: Rect, form: &ConfigForm, l: &L10n) {
    let r = centered(area, 80, 16);
    f.render_widget(Clear, r);
    f.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .title(l.config_form_title.to_string()),
        r,
    );
    let inner = Rect {
        x: r.x + 1,
        y: r.y + 1,
        width: r.width - 2,
        height: r.height - 2,
    };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);
    field_box(f, rows[0], l.server_dir_field, &form.server_dir, form.field == 0);
    field_box(f, rows[1], l.server_cmd_field, &form.server_cmd, form.field == 1);
    field_box(
        f,
        rows[2],
        l.scenarios_dir_field,
        &form.scenarios_dir,
        form.field == 2,
    );
    field_box(f, rows[3], l.results_dir_field, &form.results_dir, form.field == 3);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            l.config_form_hint.to_string(),
            Style::default().fg(Color::DarkGray),
        ))),
        rows[5],
    );
    let (buf, cur) = match form.field {
        0 => (&form.server_dir, form.cur[0]),
        1 => (&form.server_cmd, form.cur[1]),
        2 => (&form.scenarios_dir, form.cur[2]),
        3 => (&form.results_dir, form.cur[3]),
        _ => return,
    };
    place_cursor(f, rows[form.field], buf, cur);
}

fn field_box(f: &mut ratatui::Frame, area: Rect, title: &str, text: &str, focus: bool) {
    let border = if focus {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    f.render_widget(
        Paragraph::new(text.to_string()).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border)
                .title(title.to_string()),
        ),
        area,
    );
}

fn place_cursor(f: &mut ratatui::Frame, area: Rect, text: &str, byte_idx: usize) {
    let visible = &text[..byte_idx.min(text.len())];
    let col = area.x + 1 + visible.chars().count() as u16;
    f.set_cursor_position((col.min(area.x + area.width - 2), area.y + 1));
}

fn draw_wizard(f: &mut ratatui::Frame, area: Rect, w: &Wizard, app: &App) {
    let l = app.tr();
    let r = centered(area, 80, 22);
    f.render_widget(Clear, r);
    let title = format!(
        "{} — {}/4",
        l.wizard_title,
        w.page + 1
    );
    f.render_widget(
        Block::default().borders(Borders::ALL).title(title),
        r,
    );
    let inner = Rect {
        x: r.x + 1,
        y: r.y + 1,
        width: r.width - 2,
        height: r.height - 2,
    };
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(inner);
    match w.page {
        0 => {
            let lines: Vec<Line<'static>> = l
                .wizard_welcome
                .iter()
                .map(|s| Line::from(s.to_string()))
                .collect();
            f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), rows[0]);
        }
        1 => {
            let sub = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Length(3),
                    Constraint::Min(1),
                ])
                .split(rows[0]);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    l.wizard_dir_hint.to_string(),
                    Style::default().fg(Color::DarkGray),
                ))),
                sub[0],
            );
            field_box(f, sub[1], l.wizard_dir_title, &w.dir_buf, true);
            place_cursor(f, sub[1], &w.dir_buf, w.dir_cur);
        }
        2 => {
            let sub = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1),
                    Constraint::Length((PRESETS.len() as u16) + 2),
                    Constraint::Length(3),
                    Constraint::Min(0),
                ])
                .split(rows[0]);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    l.wizard_cmd_hint.to_string(),
                    Style::default().fg(Color::DarkGray),
                ))),
                sub[0],
            );
            let preset_items: Vec<ListItem> = PRESETS
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let prefix = if i == w.preset_idx { "▶ " } else { "  " };
                    ListItem::new(Line::from(vec![
                        Span::raw(prefix),
                        Span::styled(p.label, Style::default().fg(Color::Cyan)),
                        Span::raw(format!("  {}", p.cmd)),
                    ]))
                })
                .collect();
            let preset_border = if !w.cmd_edit_focus {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            f.render_widget(
                List::new(preset_items).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .border_style(preset_border)
                        .title(l.wizard_preset_title.to_string()),
                ),
                sub[1],
            );
            field_box(f, sub[2], l.wizard_cmd_title, &w.cmd_buf, w.cmd_edit_focus);
            if w.cmd_edit_focus {
                place_cursor(f, sub[2], &w.cmd_buf, w.cmd_cur);
            }
        }
        3 => {
            let sub = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(1), Constraint::Min(1)])
                .split(rows[0]);
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    l.wizard_scenarios_hint.to_string(),
                    Style::default().fg(Color::DarkGray),
                ))),
                sub[0],
            );
            if app.scenarios.is_empty() {
                f.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        l.wizard_scenarios_empty.to_string(),
                        Style::default().fg(Color::DarkGray),
                    )))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(l.wizard_scenarios_title.to_string()),
                    ),
                    sub[1],
                );
            } else {
                let items: Vec<ListItem> = app
                    .scenarios
                    .iter()
                    .map(|s| {
                        let is_sel = w.scenario_selected.contains(&s.name);
                        let mark = if is_sel { "[x]" } else { "[ ]" };
                        let color = if is_sel { Color::Green } else { Color::DarkGray };
                        ListItem::new(Line::from(vec![
                            Span::styled(format!("{mark} "), Style::default().fg(color)),
                            Span::styled(s.name.clone(), Style::default().fg(Color::Cyan)),
                            Span::styled(
                                format!("  ({} steps)", s.steps.len()),
                                Style::default().fg(Color::DarkGray),
                            ),
                        ]))
                    })
                    .collect();
                let list = List::new(items)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(l.wizard_scenarios_title.to_string()),
                    )
                    .highlight_style(
                        Style::default()
                            .bg(Color::DarkGray)
                            .add_modifier(Modifier::BOLD),
                    )
                    .highlight_symbol("> ");
                let mut state = ListState::default();
                state.select(Some(w.scenario_sel.min(app.scenarios.len() - 1)));
                f.render_stateful_widget(list, sub[1], &mut state);
            }
        }
        _ => {}
    }
    let hint = if w.page == 3 {
        l.wizard_finish_hint
    } else {
        l.wizard_nav_hint
    };
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            hint.to_string(),
            Style::default().fg(Color::DarkGray),
        ))),
        rows[1],
    );
}
