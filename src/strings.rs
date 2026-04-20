use crate::core::Lang;

pub struct L10n {
    pub banner: &'static str,
    pub running: &'static str,
    pub stopped: &'static str,
    pub config_prefix: &'static str,

    pub server_panel: &'static str,
    pub dir_prefix: &'static str,
    pub cmd_prefix: &'static str,
    pub none_value: &'static str,
    pub press_c_hint: &'static str,

    pub scenarios_title: &'static str, // "Scenarios ({})"
    pub selected_tag: &'static str,
    pub no_scenarios_loaded: &'static str,
    pub progress_idle: &'static str,
    pub progress_title: &'static str, // "Progress"

    pub output_following: &'static str,
    pub output_scroll: &'static str, // "Output (scroll {})"

    pub send_active: &'static str,
    pub send_inactive: &'static str,

    pub hint_start: &'static str,
    pub hint_stop: &'static str,
    pub hint_config: &'static str,
    pub hint_run: &'static str,
    pub hint_toggle: &'static str,
    pub hint_export: &'static str,
    pub hint_input: &'static str,
    pub hint_focus: &'static str,
    pub hint_lang: &'static str,
    pub hint_help: &'static str,
    pub hint_quit: &'static str,

    pub config_form_title: &'static str,
    pub server_dir_field: &'static str,
    pub server_cmd_field: &'static str,
    pub scenarios_dir_field: &'static str,
    pub results_dir_field: &'static str,
    pub config_form_hint: &'static str,

    pub wizard_title: &'static str,
    pub wizard_welcome: &'static [&'static str],
    pub wizard_dir_title: &'static str,
    pub wizard_dir_hint: &'static str,
    pub wizard_cmd_title: &'static str,
    pub wizard_cmd_hint: &'static str,
    pub wizard_preset_title: &'static str,
    pub wizard_scenarios_title: &'static str,
    pub wizard_scenarios_hint: &'static str,
    pub wizard_scenarios_empty: &'static str,
    pub wizard_nav_hint: &'static str,
    pub wizard_finish_hint: &'static str,

    pub help_title: &'static str,
    pub help_body: &'static [&'static str],
    pub press_any_to_close: &'static str,

    pub error_title: &'static str,
    pub press_esc_or_enter: &'static str,

    pub err_server_cmd_empty: &'static str,
    pub err_server_dir_missing: &'static str,

    pub tpl_loaded_config: &'static str,        // "loaded config: {}"
    pub tpl_started_at: &'static str,           // "started: {} (dir={})"
    pub tpl_spawn_failed: &'static str,         // "spawn failed: {}"
    pub tpl_save_failed: &'static str,          // "save failed: {}"
    pub tpl_server_exited: &'static str,        // "server exited (code={})"
    pub tpl_scenario_start: &'static str,       // "scenario start: {}"
    pub tpl_scenario_done: &'static str,        // "scenario done: {} ({} samples)"
    pub tpl_step_start: &'static str,           // "→ {}"
    pub tpl_step_done: &'static str,            // "✓ {}"
    pub tpl_sample: &'static str,               // "sample: {}"
    pub tpl_runner_error: &'static str,         // "runner: {}"
    pub tpl_runner_info: &'static str,          // "runner: {}"
    pub tpl_export_ok: &'static str,            // "exported: {} / {}"
    pub tpl_export_failed: &'static str,        // "export failed: {}"
    pub tpl_scenario_load_failed: &'static str, // "load {}: {}"
    pub tpl_scenarios_loaded: &'static str,     // "loaded {} scenario(s) from {}"
    pub tpl_run_started: &'static str,          // "running {} scenario(s)"

    pub sent_stop: &'static str,
    pub stdin_unavailable: &'static str,
    pub server_already_running: &'static str,
    pub server_cmd_empty_hint: &'static str,
    pub no_server_running: &'static str,
    pub closed_server_stdin: &'static str,
    pub no_server_hint: &'static str,
    pub config_updated_hint: &'static str,
    pub run_already_active: &'static str,
    pub run_no_selection: &'static str,
    pub no_samples_to_export: &'static str,
    pub summary_title: &'static str,
}

pub const EN: L10n = L10n {
    banner: " Observer ",
    running: "● running",
    stopped: "○ stopped",
    config_prefix: "  config: ",

    server_panel: "Server",
    dir_prefix: "dir: ",
    cmd_prefix: "cmd: ",
    none_value: "<none>",
    press_c_hint: "(press `c` to edit)",

    scenarios_title: "Scenarios ({})",
    selected_tag: "[x]",
    no_scenarios_loaded: "(no scenarios — drop `.mcb` files into scenarios_dir)",
    progress_idle: "(idle — press `r` to run selected scenarios)",
    progress_title: "Progress",

    output_following: "Output (following)",
    output_scroll: "Output (scroll {})",

    send_active: "Send to server (Enter)",
    send_inactive: "Send to server (Enter) — no server running, press `s` to start",

    hint_start: "tart ",
    hint_stop: "top ",
    hint_config: "onfig ",
    hint_run: "un ",
    hint_toggle: "ggl sel ",
    hint_export: "xport ",
    hint_input: "nput ",
    hint_focus: " focus ",
    hint_lang: " 中/EN ",
    hint_help: " help ",
    hint_quit: "uit",

    config_form_title: "Edit config",
    server_dir_field: "server_dir (cwd when launching)",
    server_cmd_field: "server_cmd (shell-split: java -Xmx2G -jar server.jar nogui)",
    scenarios_dir_field: "scenarios_dir (directory with .mcb files)",
    results_dir_field: "results_dir (benchmark CSV/JSON output)",
    config_form_hint: "Tab: next field   Ctrl-S: save   Esc: cancel",

    wizard_title: "First-run setup",
    wizard_welcome: &[
        "Welcome to Observer — a scripted MC benchmark runner.",
        "",
        "We'll set up 3 things:",
        "  1. the server directory",
        "  2. the server startup command (Vanilla / Fabric / Forge / Paper)",
        "  3. which test scenarios (.mcb files) to run",
        "",
        "You can change all of these later with `c`.",
        "",
        "Press Enter to begin, Esc to skip (use defaults).",
    ],
    wizard_dir_title: "Server directory",
    wizard_dir_hint: "absolute path to the folder containing server.jar / eula.txt",
    wizard_cmd_title: "Startup command",
    wizard_cmd_hint: "pick a preset with ↑/↓, then edit freely; Enter goes to next page",
    wizard_preset_title: "Presets",
    wizard_scenarios_title: "Select test scenarios",
    wizard_scenarios_hint: "↑/↓ navigate · space toggle · Enter finish",
    wizard_scenarios_empty: "(no .mcb files in scenarios_dir — you can add them later)",
    wizard_nav_hint: "Enter: next   Esc: skip wizard",
    wizard_finish_hint: "Enter: save and finish   Esc: cancel",

    help_title: "Help",
    help_body: &[
        "Observer — scripted MC benchmark runner",
        "",
        "s        manually start server (optional — scenarios can `start` themselves)",
        "S        manually stop server (sends `stop`)",
        "c        edit config (dirs + startup cmd)",
        "r        run selected scenarios (each may include start/stop)",
        "space    toggle selection on highlighted scenario",
        "x        export accumulated samples to CSV+JSON",
        "i / /    focus input field",
        "Tab      cycle focus (Input / Scenarios / Log)",
        "↑/↓      navigate (Scenarios: select, Log: scroll)",
        "Enter    in Input: send to server",
        "L        toggle language (English / 中文)",
        ":quit    close server stdin (lets server finish)",
        "Ctrl-C   quit observer",
    ],
    press_any_to_close: "press any key to close",

    error_title: "Error",
    press_esc_or_enter: "press Esc or Enter to close",

    err_server_cmd_empty: "server_cmd is empty",
    err_server_dir_missing: "server_dir does not exist",

    tpl_loaded_config: "loaded config: {}",
    tpl_started_at: "started: {} (dir={})",
    tpl_spawn_failed: "spawn failed: {}",
    tpl_save_failed: "autosave failed: {}",
    tpl_server_exited: "server exited (code={})",
    tpl_scenario_start: "scenario start: {}",
    tpl_scenario_done: "scenario done: {} ({} samples)",
    tpl_step_start: "→ {}",
    tpl_step_done: "✓ {}",
    tpl_sample: "sample: {}",
    tpl_runner_error: "runner: {}",
    tpl_runner_info: "runner: {}",
    tpl_export_ok: "exported: {} / {}",
    tpl_export_failed: "export failed: {}",
    tpl_scenario_load_failed: "load {}: {}",
    tpl_scenarios_loaded: "loaded from {}: {} scenario(s)",
    tpl_run_started: "running {} scenario(s)",

    sent_stop: "sent `stop`",
    stdin_unavailable: "server stdin unavailable",
    server_already_running: "server already running",
    server_cmd_empty_hint: "server_cmd is empty — press `c` to set it",
    no_server_running: "no server running",
    closed_server_stdin: "closed server stdin",
    no_server_hint: "no server running — press `s` to start",
    config_updated_hint: "config updated",
    run_already_active: "a run is already in progress",
    run_no_selection: "no scenarios selected — press space on one",
    no_samples_to_export: "no samples to export",
    summary_title: "summary",
};

pub const ZH: L10n = L10n {
    banner: " Observer ",
    running: "● 运行中",
    stopped: "○ 已停止",
    config_prefix: "  配置: ",

    server_panel: "服务器",
    dir_prefix: "目录: ",
    cmd_prefix: "命令: ",
    none_value: "<未设置>",
    press_c_hint: "(按 `c` 编辑)",

    scenarios_title: "测试集 ({})",
    selected_tag: "[x]",
    no_scenarios_loaded: "(没有测试集 — 把 `.mcb` 文件放进 scenarios_dir)",
    progress_idle: "(空闲 — 按 `r` 运行已选测试集)",
    progress_title: "进度",

    output_following: "输出 (跟随末尾)",
    output_scroll: "输出 (上翻 {})",

    send_active: "发送到服务器 (回车)",
    send_inactive: "发送到服务器 (回车) — 服务器未启动，按 `s` 启动",

    hint_start: "启动 ",
    hint_stop: "停止 ",
    hint_config: "配置 ",
    hint_run: "运行 ",
    hint_toggle: "切选 ",
    hint_export: "导出 ",
    hint_input: "输入 ",
    hint_focus: " 焦点 ",
    hint_lang: " 中/EN ",
    hint_help: " 帮助 ",
    hint_quit: "退出",

    config_form_title: "编辑配置",
    server_dir_field: "服务器目录 (启动时作为工作目录)",
    server_cmd_field: "启动命令 (按 shell 规则拆分: java -Xmx2G -jar server.jar nogui)",
    scenarios_dir_field: "测试集目录 (存放 .mcb 文件)",
    results_dir_field: "结果目录 (benchmark 的 CSV/JSON 输出)",
    config_form_hint: "Tab: 下一字段   Ctrl-S: 保存   Esc: 取消",

    wizard_title: "首次启动设置",
    wizard_welcome: &[
        "欢迎使用 Observer — 脚本化的 MC 性能测试工具。",
        "",
        "接下来配置 3 项:",
        "  1. 服务器目录",
        "  2. 服务器启动命令 (Vanilla / Fabric / Forge / Paper)",
        "  3. 要运行的测试集 (.mcb 文件)",
        "",
        "以上都可以稍后按 `c` 再改。",
        "",
        "按 Enter 开始，按 Esc 跳过 (用默认值)。",
    ],
    wizard_dir_title: "服务器目录",
    wizard_dir_hint: "指向包含 server.jar / eula.txt 的目录绝对路径",
    wizard_cmd_title: "启动命令",
    wizard_cmd_hint: "↑/↓ 挑预置，可以自由编辑，Enter 进入下一页",
    wizard_preset_title: "预置",
    wizard_scenarios_title: "选择测试集",
    wizard_scenarios_hint: "↑/↓ 导航 · 空格切换 · Enter 完成",
    wizard_scenarios_empty: "(scenarios_dir 下没有 .mcb 文件 — 稍后再补也行)",
    wizard_nav_hint: "Enter: 下一页   Esc: 跳过引导",
    wizard_finish_hint: "Enter: 保存并完成   Esc: 取消",

    help_title: "帮助",
    help_body: &[
        "Observer — 脚本化的 MC 性能测试工具",
        "",
        "s        手动启动服务器 (可选,测试集内可用 `start` 自启)",
        "S        手动停止服务器 (发送 `stop`)",
        "c        编辑配置 (目录 + 启动命令)",
        "r        运行已选测试集 (测试集自带 start/stop)",
        "space    切换当前选中测试集的勾选",
        "x        导出累积的 samples 为 CSV+JSON",
        "i / /    聚焦输入框",
        "Tab      切换焦点 (输入 / 测试集 / 日志)",
        "↑/↓      导航 (测试集: 选中, 日志: 滚动)",
        "Enter    在输入框内: 发送到服务器",
        "L        切换语言 (English / 中文)",
        ":quit    关闭服务器的 stdin (让服务器自己收尾)",
        "Ctrl-C   退出 observer",
    ],
    press_any_to_close: "按任意键关闭",

    error_title: "错误",
    press_esc_or_enter: "按 Esc 或 Enter 关闭",

    err_server_cmd_empty: "启动命令不能为空",
    err_server_dir_missing: "服务器目录不存在",

    tpl_loaded_config: "已加载配置: {}",
    tpl_started_at: "已启动: {} (目录={})",
    tpl_spawn_failed: "启动失败: {}",
    tpl_save_failed: "自动保存失败: {}",
    tpl_server_exited: "服务器已退出 (退出码={})",
    tpl_scenario_start: "测试集开始: {}",
    tpl_scenario_done: "测试集结束: {} ({} 条 sample)",
    tpl_step_start: "→ {}",
    tpl_step_done: "✓ {}",
    tpl_sample: "采样: {}",
    tpl_runner_error: "runner: {}",
    tpl_runner_info: "runner: {}",
    tpl_export_ok: "已导出: {} / {}",
    tpl_export_failed: "导出失败: {}",
    tpl_scenario_load_failed: "加载 {}: {}",
    tpl_scenarios_loaded: "从 {} 加载了 {} 个测试集",
    tpl_run_started: "开始运行 {} 个测试集",

    sent_stop: "已发送 `stop`",
    stdin_unavailable: "服务器 stdin 不可用",
    server_already_running: "服务器已经在运行",
    server_cmd_empty_hint: "启动命令为空 — 按 `c` 设置",
    no_server_running: "没有正在运行的服务器",
    closed_server_stdin: "已关闭服务器的 stdin",
    no_server_hint: "没有运行中的服务器 — 按 `s` 启动",
    config_updated_hint: "配置已更新",
    run_already_active: "已有测试集正在运行",
    run_no_selection: "未选中任何测试集 — 在条目上按空格",
    no_samples_to_export: "没有可导出的 samples",
    summary_title: "统计摘要",
};

pub fn t(lang: Lang) -> &'static L10n {
    match lang {
        Lang::En => &EN,
        Lang::Zh => &ZH,
    }
}

pub fn render_tpl1(template: &str, arg: &str) -> String {
    template.replacen("{}", arg, 1)
}

pub fn render_tpl2(template: &str, a: &str, b: &str) -> String {
    template.replacen("{}", a, 1).replacen("{}", b, 1)
}

pub struct Preset {
    pub label: &'static str,
    pub cmd: &'static str,
}

pub const PRESETS: &[Preset] = &[
    Preset {
        label: "Vanilla",
        cmd: "java -Xmx4G -Xms4G -jar server.jar nogui",
    },
    Preset {
        label: "Fabric",
        cmd: "java -Xmx4G -Xms4G -jar fabric-server-launch.jar nogui",
    },
    Preset {
        label: "Paper / Purpur",
        cmd: "java -Xmx4G -Xms4G -jar paper.jar --nogui",
    },
    Preset {
        label: "Forge",
        cmd: "java -Xmx4G -Xms4G -jar forge-server.jar nogui",
    },
];
