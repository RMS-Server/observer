use std::collections::VecDeque;
use std::io::{self, Stdout, stdout};
use std::sync::mpsc;
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

use crate::core::{self, Config, Event as SEvent, Lang, MatchMode, RuleDef, Session};
use crate::strings::{L10n, render_tpl1, render_tpl2, t};

const LOG_CAP: usize = 5000;
const DEFAULT_CFG: &str = "./wrapper.json";

type Term = Terminal<CrosstermBackend<Stdout>>;

#[derive(Clone, Copy, PartialEq)]
enum Focus {
    Input,
    Rules,
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
    Rule,
    Info,
    Error,
}

const RULE_FORM_FIELDS: usize = 6;

struct RuleForm {
    editing: Option<usize>,
    field: usize,
    pattern: String,
    match_mode: MatchMode,
    commands: String,
    once: bool,
    delay_ms: String,
    gap_ms: String,
    pat_cur: usize,
    cmd_cur: usize,
    d_cur: usize,
    g_cur: usize,
}

impl RuleForm {
    fn new(editing: Option<usize>, rule: &RuleDef) -> Self {
        let cmds = rule.commands.join("\n");
        let delay = rule.delay_ms.to_string();
        let gap = rule.gap_ms.to_string();
        Self {
            editing,
            field: 0,
            pat_cur: rule.pattern.len(),
            cmd_cur: cmds.len(),
            d_cur: delay.len(),
            g_cur: gap.len(),
            pattern: rule.pattern.clone(),
            match_mode: rule.match_mode,
            commands: cmds,
            once: rule.once,
            delay_ms: delay,
            gap_ms: gap,
        }
    }

    fn new_blank() -> Self {
        Self::new(None, &RuleDef::default())
    }

    fn to_rule(&self, lang: Lang) -> Result<RuleDef, String> {
        let l = t(lang);
        if self.pattern.is_empty() {
            return Err(l.err_pattern_empty.into());
        }
        let commands: Vec<String> = self
            .commands
            .split('\n')
            .map(|s| s.trim_end_matches('\r').to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if commands.is_empty() {
            return Err(l.err_commands_empty.into());
        }
        if !self.delay_ms.trim().is_empty() && self.delay_ms.trim().parse::<u64>().is_err() {
            return Err(l.err_delay_not_int.into());
        }
        if !self.gap_ms.trim().is_empty() && self.gap_ms.trim().parse::<u64>().is_err() {
            return Err(l.err_gap_not_int.into());
        }
        let def = RuleDef {
            pattern: self.pattern.clone(),
            commands,
            once: self.once,
            delay_ms: self.delay_ms.trim().parse().unwrap_or(0),
            gap_ms: self.gap_ms.trim().parse().unwrap_or(0),
            match_mode: self.match_mode,
        };
        core::validate_rule(&def)?;
        Ok(def)
    }
}

const MATCH_MODES: [MatchMode; 4] = [
    MatchMode::Contains,
    MatchMode::Exact,
    MatchMode::Glob,
    MatchMode::Regex,
];

fn mode_index(m: MatchMode) -> usize {
    MATCH_MODES.iter().position(|x| *x == m).unwrap_or(0)
}

fn mode_label(m: MatchMode) -> &'static str {
    match m {
        MatchMode::Contains => "contains",
        MatchMode::Exact => "exact",
        MatchMode::Glob => "glob",
        MatchMode::Regex => "regex",
    }
}


struct ConfigForm {
    field: usize,
    server_dir: String,
    server_cmd: String,
    dir_cur: usize,
    cmd_cur: usize,
}

impl ConfigForm {
    fn new(cfg: &Config) -> Self {
        let dir = cfg.server_dir.clone().unwrap_or_default();
        let cmd = cfg.server_cmd.join(" ");
        Self {
            field: 0,
            dir_cur: dir.len(),
            cmd_cur: cmd.len(),
            server_dir: dir,
            server_cmd: cmd,
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
        Ok(())
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
    RuleForm(RuleForm),
    ConfigForm(ConfigForm),
    Error(String),
    Help,
}

struct App {
    config: Config,
    config_path: String,
    session: Option<Session>,
    events: Option<mpsc::Receiver<SEvent>>,
    log: VecDeque<LogLine>,
    log_scroll: usize,
    input: String,
    input_cur: usize,
    focus: Focus,
    rule_sel: usize,
    modal: Option<Modal>,
    status: String,
    status_until: Option<Instant>,
    should_quit: bool,
}

impl App {
    fn tr(&self) -> &'static L10n {
        t(self.config.lang)
    }

    fn new(config_path: Option<&str>) -> io::Result<Self> {
        let path = config_path.unwrap_or(DEFAULT_CFG).to_string();
        let config = match core::load_config(&path) {
            Ok(c) => c,
            Err(_) => Config::default(),
        };
        let mut app = Self {
            config,
            config_path: path,
            session: None,
            events: None,
            log: VecDeque::new(),
            log_scroll: 0,
            input: String::new(),
            input_cur: 0,
            focus: Focus::Input,
            rule_sel: 0,
            modal: None,
            status: String::new(),
            status_until: None,
            should_quit: false,
        };
        let msg = render_tpl1(app.tr().tpl_loaded_config, &app.config_path);
        app.info(msg);
        Ok(app)
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
        let Some(rx) = &self.events else { return };
        let mut batch = Vec::new();
        while let Ok(ev) = rx.try_recv() {
            batch.push(ev);
        }
        for ev in batch {
            match ev {
                SEvent::Stdout(l) => self.push_log(LogKind::Out, l),
                SEvent::Stderr(l) => self.push_log(LogKind::Err, l),
                SEvent::RuleMatch {
                    pattern,
                    count,
                    delay_ms,
                    gap_ms,
                } => self.push_log(
                    LogKind::Rule,
                    format!(
                        "match /{pattern}/ -> {count} cmd(s) (delay {delay_ms}ms, gap {gap_ms}ms)"
                    ),
                ),
                SEvent::RuleSent(c) => self.push_log(LogKind::Rule, format!("sent: {c}")),
                SEvent::Exited(code) => {
                    self.session = None;
                    self.events = None;
                    let msg = render_tpl1(
                        self.tr().tpl_server_exited,
                        &code.unwrap_or(-1).to_string(),
                    );
                    self.info(msg);
                }
            }
        }
    }

    fn start_server(&mut self) {
        if self.session.is_some() {
            self.status(self.tr().server_already_running.into());
            return;
        }
        if self.config.server_cmd.is_empty() {
            self.error(self.tr().server_cmd_empty_hint.into());
            return;
        }
        match Session::spawn(&self.config) {
            Ok((s, rx)) => {
                self.session = Some(s);
                self.events = Some(rx);
                let cmd = self.config.server_cmd.join(" ");
                let dir = self.config.server_dir.clone().unwrap_or_else(|| ".".into());
                let msg = render_tpl2(self.tr().tpl_started_at, &cmd, &dir);
                self.info(msg);
            }
            Err(e) => {
                let msg = render_tpl1(self.tr().tpl_spawn_failed, &e.to_string());
                self.error(msg);
            }
        }
    }

    fn stop_server(&mut self) {
        match self.session.as_ref() {
            Some(s) => {
                if s.send_cmd("stop") {
                    self.info(self.tr().sent_stop.into());
                } else {
                    self.error(self.tr().stdin_unavailable.into());
                }
            }
            None => self.status(self.tr().no_server_running.into()),
        }
    }

    fn send_input(&mut self) {
        let line = std::mem::take(&mut self.input);
        self.input_cur = 0;
        if line.is_empty() {
            return;
        }
        if line == ":quit" {
            if let Some(s) = &self.session {
                s.close_stdin();
                self.info(self.tr().closed_server_stdin.into());
            }
            return;
        }
        match self.session.as_ref() {
            Some(s) => {
                if !s.send_cmd(&line) {
                    self.error(self.tr().stdin_unavailable.into());
                }
            }
            None => self.error(self.tr().no_server_hint.into()),
        }
    }

    fn save_config(&mut self) {
        match core::save_config(&self.config_path, &self.config) {
            Ok(_) => {
                let msg = render_tpl1(self.tr().tpl_saved_config_to, &self.config_path);
                self.info(msg);
            }
            Err(e) => {
                let msg = render_tpl1(self.tr().tpl_save_failed, &e.to_string());
                self.error(msg);
            }
        }
    }

    fn delete_rule(&mut self) {
        if self.rule_sel < self.config.rules.len() {
            let pat = self.config.rules[self.rule_sel].pattern.clone();
            self.config.rules.remove(self.rule_sel);
            if self.rule_sel >= self.config.rules.len() && self.rule_sel > 0 {
                self.rule_sel -= 1;
            }
            let msg = render_tpl1(self.tr().tpl_deleted_rule, &pat);
            self.info(msg);
        }
    }

    fn toggle_lang(&mut self) {
        self.config.lang = match self.config.lang {
            Lang::En => Lang::Zh,
            Lang::Zh => Lang::En,
        };
    }

    fn run(&mut self, terminal: &mut Term) -> io::Result<()> {
        let tick = Duration::from_millis(80);
        while !self.should_quit {
            self.drain_session();
            terminal.draw(|f| draw(f, self))?;
            if event::poll(tick)? {
                match event::read()? {
                    CEvent::Key(k) if k.kind == KeyEventKind::Press => self.handle_key(k),
                    _ => {}
                }
            }
            if let Some(deadline) = self.status_until {
                if Instant::now() >= deadline {
                    self.status.clear();
                    self.status_until = None;
                }
            }
        }
        if let Some(s) = &self.session {
            let _ = s.send_cmd("stop");
        }
        Ok(())
    }

    fn handle_key(&mut self, k: KeyEvent) {
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
                KeyCode::Tab => self.focus = Focus::Rules,
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
                    Focus::Input => Focus::Rules,
                    Focus::Rules => Focus::Log,
                    Focus::Log => Focus::Input,
                };
            }
            KeyCode::Char('i') | KeyCode::Char('/') => self.focus = Focus::Input,
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('s') => self.start_server(),
            KeyCode::Char('S') => self.stop_server(),
            KeyCode::Char('a') => {
                self.modal = Some(Modal::RuleForm(RuleForm::new_blank()));
            }
            KeyCode::Char('e') => {
                if self.config.rules.is_empty() {
                    self.status(self.tr().no_rules_to_edit.into());
                } else if self.rule_sel < self.config.rules.len() {
                    let r = self.config.rules[self.rule_sel].clone();
                    self.modal = Some(Modal::RuleForm(RuleForm::new(Some(self.rule_sel), &r)));
                }
            }
            KeyCode::Char('d') => {
                if self.config.rules.is_empty() {
                    self.status(self.tr().no_rules_to_delete.into());
                } else {
                    self.delete_rule();
                }
            }
            KeyCode::Char('j') => {
                if self.focus != Focus::Input && self.rule_sel + 1 < self.config.rules.len() {
                    self.rule_sel += 1;
                }
            }
            KeyCode::Char('k') => {
                if self.focus != Focus::Input && self.rule_sel > 0 {
                    self.rule_sel -= 1;
                }
            }
            KeyCode::Char('c') => {
                self.modal = Some(Modal::ConfigForm(ConfigForm::new(&self.config)));
            }
            KeyCode::Char('w') => self.save_config(),
            KeyCode::Char('L') => self.toggle_lang(),
            KeyCode::Char('?') => self.modal = Some(Modal::Help),
            KeyCode::Up => match self.focus {
                Focus::Rules => {
                    if self.rule_sel > 0 {
                        self.rule_sel -= 1;
                    }
                }
                Focus::Log => self.log_scroll = self.log_scroll.saturating_add(1),
                _ => {}
            },
            KeyCode::Down => match self.focus {
                Focus::Rules => {
                    if self.rule_sel + 1 < self.config.rules.len() {
                        self.rule_sel += 1;
                    }
                }
                Focus::Log => self.log_scroll = self.log_scroll.saturating_sub(1),
                _ => {}
            },
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

    fn handle_modal_key(&mut self, k: KeyEvent) {
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        match self.modal.as_mut() {
            Some(Modal::Error(_)) | Some(Modal::Help) => {
                if matches!(k.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q')) {
                    self.modal = None;
                }
            }
            Some(Modal::RuleForm(f)) => {
                if k.code == KeyCode::Esc {
                    self.modal = None;
                    return;
                }
                if ctrl && k.code == KeyCode::Char('s') {
                    let lang = self.config.lang;
                    match f.to_rule(lang) {
                        Ok(r) => {
                            match f.editing {
                                Some(i) => self.config.rules[i] = r,
                                None => self.config.rules.push(r),
                            }
                            self.modal = None;
                            self.info(self.tr().rule_saved_hint.into());
                        }
                        Err(e) => self.modal = Some(Modal::Error(e)),
                    }
                    return;
                }
                if k.code == KeyCode::Tab {
                    f.field = (f.field + 1) % RULE_FORM_FIELDS;
                    return;
                }
                if k.code == KeyCode::BackTab {
                    f.field = (f.field + RULE_FORM_FIELDS - 1) % RULE_FORM_FIELDS;
                    return;
                }
                match f.field {
                    0 => edit_single(&mut f.pattern, &mut f.pat_cur, k),
                    1 => match k.code {
                        KeyCode::Left | KeyCode::Char('h') => {
                            let i = mode_index(f.match_mode);
                            f.match_mode = MATCH_MODES[(i + MATCH_MODES.len() - 1) % MATCH_MODES.len()];
                        }
                        KeyCode::Right | KeyCode::Char('l') | KeyCode::Char(' ') => {
                            let i = mode_index(f.match_mode);
                            f.match_mode = MATCH_MODES[(i + 1) % MATCH_MODES.len()];
                        }
                        KeyCode::Char('1') => f.match_mode = MatchMode::Contains,
                        KeyCode::Char('2') => f.match_mode = MatchMode::Exact,
                        KeyCode::Char('3') => f.match_mode = MatchMode::Glob,
                        KeyCode::Char('4') => f.match_mode = MatchMode::Regex,
                        _ => {}
                    },
                    2 => edit_multi(&mut f.commands, &mut f.cmd_cur, k),
                    3 => {
                        if matches!(k.code, KeyCode::Char(' ') | KeyCode::Enter) {
                            f.once = !f.once;
                        }
                    }
                    4 => edit_digits(&mut f.delay_ms, &mut f.d_cur, k),
                    5 => edit_digits(&mut f.gap_ms, &mut f.g_cur, k),
                    _ => {}
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
                            self.config = cfg;
                            self.modal = None;
                            self.info(self.tr().config_updated_hint.into());
                        }
                        Err(e) => self.modal = Some(Modal::Error(e)),
                    }
                    return;
                }
                if k.code == KeyCode::Tab || k.code == KeyCode::BackTab {
                    f.field = 1 - f.field;
                    return;
                }
                match f.field {
                    0 => edit_single(&mut f.server_dir, &mut f.dir_cur, k),
                    1 => edit_single(&mut f.server_cmd, &mut f.cmd_cur, k),
                    _ => {}
                }
            }
            None => {}
        }
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

fn edit_multi(buf: &mut String, cur: &mut usize, k: KeyEvent) {
    match k.code {
        KeyCode::Enter => {
            buf.insert(*cur, '\n');
            *cur += 1;
        }
        _ => edit_single(buf, cur, k),
    }
}

fn edit_digits(buf: &mut String, cur: &mut usize, k: KeyEvent) {
    match k.code {
        KeyCode::Char(c) if !c.is_ascii_digit() => {}
        _ => edit_single(buf, cur, k),
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

    if let Some(m) = &app.modal {
        let l = app.tr();
        match m {
            Modal::Help => draw_help(f, area, l),
            Modal::Error(msg) => draw_error(f, area, msg, l),
            Modal::RuleForm(form) => draw_rule_form(f, area, form, l),
            Modal::ConfigForm(form) => draw_config_form(f, area, form, l),
        }
    }
}

fn draw_title(f: &mut ratatui::Frame, area: Rect, app: &App) {
    let l = app.tr();
    let running = if app.session.is_some() {
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
        .constraints([Constraint::Length(5), Constraint::Min(3)])
        .split(area);

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
        rows[0],
    );

    let rules_focused = app.focus == Focus::Rules;
    let items: Vec<ListItem> = app
        .config
        .rules
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let head = Line::from(vec![
                Span::styled(format!("{i:>2} "), Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{} ", mode_label(r.match_mode)),
                    Style::default().fg(Color::Yellow),
                ),
                Span::styled(
                    format!("{:?}", r.pattern),
                    Style::default().fg(Color::Cyan),
                ),
                Span::raw(if r.once {
                    format!("  {}", l.once_tag)
                } else {
                    String::new()
                }),
            ]);
            let tail = Line::from(vec![
                Span::styled("   -> ", Style::default().fg(Color::DarkGray)),
                Span::raw(r.commands.join(" ; ")),
            ]);
            let meta = if r.delay_ms > 0 || r.gap_ms > 0 {
                Some(Line::from(Span::styled(
                    format!("   delay {}ms, gap {}ms", r.delay_ms, r.gap_ms),
                    Style::default().fg(Color::DarkGray),
                )))
            } else {
                None
            };
            let mut v = vec![head, tail];
            if let Some(m) = meta {
                v.push(m);
            }
            ListItem::new(v)
        })
        .collect();
    let title = render_tpl1(l.rules_title, &app.config.rules.len().to_string());
    let border_style = if rules_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
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
    if !app.config.rules.is_empty() {
        state.select(Some(app.rule_sel.min(app.config.rules.len() - 1)));
    }
    f.render_stateful_widget(list, rows[1], &mut state);
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
        LogKind::Rule => ("[RULE] ", Color::Magenta),
        LogKind::Info => ("[wrapper] ", Color::Cyan),
        LogKind::Error => ("[wrapper ERR] ", Color::Red),
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
    let title = if app.session.is_some() {
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
        hint("a", l.hint_add),
        hint("e", l.hint_edit),
        hint("d", l.hint_delete),
        hint("w", l.hint_write),
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
    let r = centered(area, 60, h);
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

fn draw_rule_form(f: &mut ratatui::Frame, area: Rect, form: &RuleForm, l: &L10n) {
    let r = centered(area, 84, 24);
    f.render_widget(Clear, r);
    let title = match form.editing {
        Some(i) => render_tpl1(l.edit_rule_title, &i.to_string()),
        None => l.add_rule_title.to_string(),
    };
    f.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .title(title)
            .style(Style::default().fg(Color::White)),
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
            Constraint::Length(3), // 0 pattern
            Constraint::Length(3), // 1 match mode
            Constraint::Min(4),    // 2 commands
            Constraint::Length(1), // 3 once
            Constraint::Length(3), // 4 delay
            Constraint::Length(3), // 5 gap
            Constraint::Length(1), // hint
        ])
        .split(inner);

    let pattern_title = render_tpl1(l.pattern_field, mode_label(form.match_mode));
    field_box(f, rows[0], &pattern_title, &form.pattern, form.field == 0);
    draw_mode_picker(f, rows[1], form.match_mode, form.field == 1, l);
    field_box_multi(f, rows[2], l.commands_field, &form.commands, form.field == 2);
    let once_style = if form.field == 3 {
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(l.once_prompt.to_string(), Style::default().fg(Color::DarkGray)),
            Span::styled(if form.once { "[x]" } else { "[ ]" }, once_style),
        ])),
        rows[3],
    );
    field_box(f, rows[4], l.delay_field, &form.delay_ms, form.field == 4);
    field_box(f, rows[5], l.gap_field, &form.gap_ms, form.field == 5);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            l.rule_form_hint.to_string(),
            Style::default().fg(Color::DarkGray),
        ))),
        rows[6],
    );

    set_form_cursor(f, form, &rows);
}

fn draw_mode_picker(f: &mut ratatui::Frame, area: Rect, mode: MatchMode, focused: bool, l: &L10n) {
    let border = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let mut spans: Vec<Span<'static>> = Vec::new();
    for m in MATCH_MODES {
        let selected = m == mode;
        let s = if selected {
            Style::default()
                .bg(Color::Cyan)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        spans.push(Span::styled(format!(" {} ", mode_label(m)), s));
        spans.push(Span::raw(" "));
    }
    let help = match mode {
        MatchMode::Contains => l.mode_help_contains,
        MatchMode::Exact => l.mode_help_exact,
        MatchMode::Glob => l.mode_help_glob,
        MatchMode::Regex => l.mode_help_regex,
    };
    spans.push(Span::styled(
        format!("— {}", help),
        Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC),
    ));
    f.render_widget(
        Paragraph::new(Line::from(spans)).block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border)
                .title(l.mode_picker_title.to_string()),
        ),
        area,
    );
}

fn set_form_cursor(f: &mut ratatui::Frame, form: &RuleForm, rows: &[Rect]) {
    match form.field {
        0 => place_cursor(f, rows[0], &form.pattern, form.pat_cur),
        2 => place_cursor_multi(f, rows[2], &form.commands, form.cmd_cur),
        4 => place_cursor(f, rows[4], &form.delay_ms, form.d_cur),
        5 => place_cursor(f, rows[5], &form.gap_ms, form.g_cur),
        _ => {}
    }
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

fn field_box_multi(f: &mut ratatui::Frame, area: Rect, title: &str, text: &str, focus: bool) {
    let border = if focus {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    f.render_widget(
        Paragraph::new(text.to_string()).wrap(Wrap { trim: false }).block(
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

fn place_cursor_multi(f: &mut ratatui::Frame, area: Rect, text: &str, byte_idx: usize) {
    let before = &text[..byte_idx.min(text.len())];
    let mut row = 0u16;
    let mut col = 0u16;
    for c in before.chars() {
        if c == '\n' {
            row += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    let max_x = area.x + area.width - 2;
    let max_y = area.y + area.height - 2;
    f.set_cursor_position(((area.x + 1 + col).min(max_x), (area.y + 1 + row).min(max_y)));
}

fn draw_config_form(f: &mut ratatui::Frame, area: Rect, form: &ConfigForm, l: &L10n) {
    let r = centered(area, 80, 12);
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
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);
    field_box(f, rows[0], l.server_dir_field, &form.server_dir, form.field == 0);
    field_box(f, rows[1], l.server_cmd_field, &form.server_cmd, form.field == 1);
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            l.config_form_hint.to_string(),
            Style::default().fg(Color::DarkGray),
        ))),
        rows[3],
    );
    match form.field {
        0 => place_cursor(f, rows[0], &form.server_dir, form.dir_cur),
        1 => place_cursor(f, rows[1], &form.server_cmd, form.cmd_cur),
        _ => {}
    }
}
