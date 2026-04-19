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

    pub rules_title: &'static str, // "Rules ({})"
    pub once_tag: &'static str,

    pub output_following: &'static str,
    pub output_scroll: &'static str, // "Output (scroll {})"

    pub send_active: &'static str,
    pub send_inactive: &'static str,

    pub hint_start: &'static str,
    pub hint_stop: &'static str,
    pub hint_config: &'static str,
    pub hint_add: &'static str,
    pub hint_edit: &'static str,
    pub hint_delete: &'static str,
    pub hint_input: &'static str,
    pub hint_focus: &'static str,
    pub hint_lang: &'static str,
    pub hint_help: &'static str,
    pub hint_quit: &'static str,

    pub add_rule_title: &'static str,
    pub edit_rule_title: &'static str, // "Edit rule #{}"
    pub pattern_field: &'static str,   // "pattern ({})"
    pub mode_picker_title: &'static str,
    pub commands_field: &'static str,
    pub once_prompt: &'static str,
    pub delay_field: &'static str,
    pub gap_field: &'static str,
    pub rule_form_hint: &'static str,

    pub mode_help_contains: &'static str,
    pub mode_help_exact: &'static str,
    pub mode_help_glob: &'static str,
    pub mode_help_regex: &'static str,

    pub config_form_title: &'static str,
    pub server_dir_field: &'static str,
    pub server_cmd_field: &'static str,
    pub config_form_hint: &'static str,

    pub help_title: &'static str,
    pub help_body: &'static [&'static str],
    pub press_any_to_close: &'static str,

    pub error_title: &'static str,
    pub press_esc_or_enter: &'static str,

    pub err_pattern_empty: &'static str,
    pub err_commands_empty: &'static str,
    pub err_delay_not_int: &'static str,
    pub err_gap_not_int: &'static str,
    pub err_server_cmd_empty: &'static str,

    // templates for parametric messages
    pub tpl_loaded_config: &'static str,       // "loaded config: {p}"
    pub tpl_started_at: &'static str,          // "started: {cmd} (dir={dir})"
    pub tpl_spawn_failed: &'static str,        // "spawn failed: {e}"
    pub tpl_save_failed: &'static str,         // "save failed: {e}"
    pub tpl_deleted_rule: &'static str,        // "deleted rule /{p}/"
    pub tpl_server_exited: &'static str,       // "server exited (code={c})"
    pub sent_stop: &'static str,
    pub stdin_unavailable: &'static str,
    pub server_already_running: &'static str,
    pub server_cmd_empty_hint: &'static str,
    pub no_server_running: &'static str,
    pub closed_server_stdin: &'static str,
    pub no_server_hint: &'static str,
    pub no_rules_to_edit: &'static str,
    pub no_rules_to_delete: &'static str,
    pub rule_saved_hint: &'static str,
    pub config_updated_hint: &'static str,
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

    rules_title: "Rules ({})",
    once_tag: "once",

    output_following: "Output (following)",
    output_scroll: "Output (scroll {})",

    send_active: "Send to server (Enter)",
    send_inactive: "Send to server (Enter) — no server running, press `s` to start",

    hint_start: "tart ",
    hint_stop: "top ",
    hint_config: "onfig ",
    hint_add: "dd ",
    hint_edit: "dit ",
    hint_delete: "el ",
    hint_input: "nput ",
    hint_focus: " focus ",
    hint_lang: " 中/EN ",
    hint_help: " help ",
    hint_quit: "uit",

    add_rule_title: "Add rule",
    edit_rule_title: "Edit rule #{}",
    pattern_field: "pattern ({})",
    mode_picker_title: "match mode (←→ or 1/2/3/4)",
    commands_field: "commands (Enter = newline, one per line)",
    once_prompt: "once (space): ",
    delay_field: "delay_ms — wait before first command",
    gap_field: "gap_ms — wait between consecutive commands",
    rule_form_hint: "Tab: next field   ←→ on match: pick mode   Ctrl-S: save   Esc: cancel",

    mode_help_contains: "substring (case-insensitive)",
    mode_help_exact: "full-line exact match",
    mode_help_glob: "wildcards: * any chars, ? one char",
    mode_help_regex: "Rust regex syntax",

    config_form_title: "Edit server config",
    server_dir_field: "server_dir (cwd when launching)",
    server_cmd_field: "server_cmd (shell-split: java -Xmx2G -jar server.jar nogui)",
    config_form_hint: "Tab: next field   Ctrl-S: save   Esc: cancel",

    help_title: "Help",
    help_body: &[
        "Observer — MC server wrapper with rule engine",
        "",
        "s        start server",
        "S        stop server (sends `stop`)",
        "c        edit server dir & cmd",
        "a        add rule",
        "e        edit selected rule",
        "d        delete selected rule",
        "i / /    focus input field",
        "Tab      cycle focus (Input / Rules / Log)",
        "↑/↓      navigate (Rules: select, Log: scroll)",
        "Enter    in Input: send to server",
        "L        toggle language (English / 中文)",
        ":quit    close server stdin (lets server finish)",
        "Ctrl-C   quit observer",
    ],
    press_any_to_close: "press any key to close",

    error_title: "Error",
    press_esc_or_enter: "press Esc or Enter to close",

    err_pattern_empty: "pattern is empty",
    err_commands_empty: "commands empty",
    err_delay_not_int: "delay_ms must be a non-negative integer",
    err_gap_not_int: "gap_ms must be a non-negative integer",
    err_server_cmd_empty: "server_cmd is empty",

    tpl_loaded_config: "loaded config: {}",
    tpl_started_at: "started: {} (dir={})",
    tpl_spawn_failed: "spawn failed: {}",
    tpl_save_failed: "autosave failed: {}",
    tpl_deleted_rule: "deleted rule /{}/",
    tpl_server_exited: "server exited (code={})",
    sent_stop: "sent `stop`",
    stdin_unavailable: "server stdin unavailable",
    server_already_running: "server already running",
    server_cmd_empty_hint: "server_cmd is empty — press `c` to set it",
    no_server_running: "no server running",
    closed_server_stdin: "closed server stdin",
    no_server_hint: "no server running — press `s` to start",
    no_rules_to_edit: "no rules to edit — press `a` to add",
    no_rules_to_delete: "no rules to delete",
    rule_saved_hint: "rule saved",
    config_updated_hint: "server config updated",
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

    rules_title: "规则 ({})",
    once_tag: "单次",

    output_following: "输出 (跟随末尾)",
    output_scroll: "输出 (上翻 {})",

    send_active: "发送到服务器 (回车)",
    send_inactive: "发送到服务器 (回车) — 服务器未启动，按 `s` 启动",

    hint_start: "启动 ",
    hint_stop: "停止 ",
    hint_config: "配置 ",
    hint_add: "添加 ",
    hint_edit: "编辑 ",
    hint_delete: "删除 ",
    hint_input: "输入 ",
    hint_focus: " 焦点 ",
    hint_lang: " 中/EN ",
    hint_help: " 帮助 ",
    hint_quit: "退出",

    add_rule_title: "添加规则",
    edit_rule_title: "编辑规则 #{}",
    pattern_field: "匹配模板 ({})",
    mode_picker_title: "匹配方式 (←→ 或 1/2/3/4)",
    commands_field: "命令列表 (回车换行，每行一条命令)",
    once_prompt: "单次触发 (空格切换): ",
    delay_field: "首次延迟(ms) — 命中后等多久发第一条命令",
    gap_field: "命令间隔(ms) — 连发多条时每两条之间的停顿",
    rule_form_hint: "Tab: 下一字段   匹配方式上用 ←→ 切换   Ctrl-S: 保存   Esc: 取消",

    mode_help_contains: "子串匹配（忽略大小写）",
    mode_help_exact: "整行精确匹配",
    mode_help_glob: "通配符：* 匹配任意字符，? 匹配单字符",
    mode_help_regex: "Rust 正则表达式",

    config_form_title: "编辑服务器配置",
    server_dir_field: "服务器目录 (启动时作为工作目录)",
    server_cmd_field: "启动命令 (按 shell 规则拆分: java -Xmx2G -jar server.jar nogui)",
    config_form_hint: "Tab: 下一字段   Ctrl-S: 保存   Esc: 取消",

    help_title: "帮助",
    help_body: &[
        "Observer — 带规则引擎的 MC 服务器 wrapper",
        "",
        "s        启动服务器",
        "S        停止服务器 (发送 `stop`)",
        "c        编辑服务器目录与启动命令",
        "a        添加规则",
        "e        编辑选中的规则",
        "d        删除选中的规则",
        "i / /    聚焦输入框",
        "Tab      切换焦点 (输入 / 规则 / 日志)",
        "↑/↓      导航 (规则: 选中, 日志: 滚动)",
        "Enter    在输入框内: 发送到服务器",
        "L        切换语言 (English / 中文)",
        ":quit    关闭服务器的 stdin (让服务器自己收尾)",
        "Ctrl-C   退出 observer",
    ],
    press_any_to_close: "按任意键关闭",

    error_title: "错误",
    press_esc_or_enter: "按 Esc 或 Enter 关闭",

    err_pattern_empty: "匹配模板不能为空",
    err_commands_empty: "命令列表不能为空",
    err_delay_not_int: "首次延迟必须是非负整数",
    err_gap_not_int: "命令间隔必须是非负整数",
    err_server_cmd_empty: "启动命令不能为空",

    tpl_loaded_config: "已加载配置: {}",
    tpl_started_at: "已启动: {} (目录={})",
    tpl_spawn_failed: "启动失败: {}",
    tpl_save_failed: "自动保存失败: {}",
    tpl_deleted_rule: "已删除规则 /{}/",
    tpl_server_exited: "服务器已退出 (退出码={})",
    sent_stop: "已发送 `stop`",
    stdin_unavailable: "服务器 stdin 不可用",
    server_already_running: "服务器已经在运行",
    server_cmd_empty_hint: "启动命令为空 — 按 `c` 设置",
    no_server_running: "没有正在运行的服务器",
    closed_server_stdin: "已关闭服务器的 stdin",
    no_server_hint: "没有运行中的服务器 — 按 `s` 启动",
    no_rules_to_edit: "没有规则可编辑 — 按 `a` 新增",
    no_rules_to_delete: "没有规则可删除",
    rule_saved_hint: "规则已保存",
    config_updated_hint: "服务器配置已更新",
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
