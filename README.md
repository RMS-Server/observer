# observer

Scripted benchmark runner and TUI wrapper for Minecraft servers. Drive a vanilla/Fabric/Paper server from the command line, run deterministic scenarios written in a tiny `.mcb` script language, and export timestamped metrics to CSV + JSON.

[English](#english) | [中文](#中文)

---

## English

### What it does

`observer` wraps a Minecraft server process (its stdin/stdout), then executes a scenario script against it:

- **start** the server with a configured command.
- **wait** for a log line matching a pattern (e.g. `Done (`).
- **send** commands into the server console.
- **grab** numeric values out of server output into named metrics.
- **loop** a body of steps for a fixed duration at a fixed interval (useful for periodic sampling like MSPT / TPS).
- **stop** the server cleanly, force-killing if it hangs past the stop window.

Each run produces a CSV and a JSON file under `results_dir`, keyed by scenario name and timestamp.

Two ways to use it:

- **TUI mode** (default) — full ratatui interface: config form, scenario picker, live log, running progress.
- **Headless mode** — `observer run ...` executes the selected scenarios and exits; suitable for CI.

### `.mcb` scenario language

```
# example_tps_stress.mcb
start                               # spawn the server
wait   Done (                       # wait for the Fabric/Paper ready line
send   /tick rate 20
sleep  2s

loop 30s every 1s
    send    /tick query rate
    grab    mspt  Average tick time: {} ms
end

stop
```

Verbs:

| Verb | Syntax | Notes |
|------|--------|-------|
| `start` | `start` | Spawns the server using `config.server_cmd` in `config.server_dir`. |
| `wait` | `wait [<timeout>] <pattern>` | Blocks until a line matches. Default timeout `120s`. |
| `send` | `send <command>` | Writes a line to server stdin. |
| `sleep` | `sleep <duration>` | Interruptible sleep. |
| `grab` | `grab <metric> <pattern>` | Reads one numeric value from the next matching line into `<metric>`. Pattern must have `{}` or a regex capture group. |
| `loop` | `loop <total> every <interval>` … `end` | Repeats body for `total`, pacing each iteration to at least `interval`. |
| `stop` | `stop` | Sends `stop`, waits up to `60s`, then kills. |

Pattern syntax:

- **Literal** (default) — substring match; regex metachars are auto-escaped. `Done (` just matches `Done (` verbatim.
- **Placeholder** — `{}` captures a decimal number (e.g. `Average: {} ms`). At most one per pattern.
- **Raw regex** — prefix with `re:` to use the full regex (e.g. `re:^\[.*\] Done \((\d+\.\d+)s\)`).

Durations: `ms`, `s`, `m` (integer only). `500ms`, `2s`, `3m`.

Comments: anything after `#` on a line is ignored.

### Config (`observer.json`)

```json
{
  "server_dir": "/path/to/server",
  "server_cmd": ["java", "-Xmx4G", "-Xms4G", "-jar", "server.jar", "nogui"],
  "lang": "en",
  "scenarios_dir": "./scenarios",
  "selected_scenarios": ["example_tps_stress"],
  "results_dir": "./results"
}
```

- `server_cmd` is the exact argv used to spawn the server.
- `selected_scenarios` is the default list for `observer run` with no arguments.
- `lang` controls TUI strings (`en` / `zh`); defaults from `$LANG` if omitted.

### Usage

```
# TUI (default)
observer
observer tui [--config <path>]

# Headless: run selected scenarios, export CSV+JSON, exit
observer run [--config <path>] [scenario-name ...]

# Scenario script tools
observer s init   <name>            # create scenarios_dir/<name>.mcb
observer s format <name-or-path>    # normalize whitespace/indentation
observer s check  <name-or-path>    # parse and report syntax errors
```

### Build

Requires Rust with `edition = "2024"` support (stable 1.85+).

```sh
cargo build --release
./target/release/observer
```

### Output

Each run writes two files to `results_dir`:

- `<scenario>_<timestamp>.csv` — one row per `grab` sample, columns are `timestamp_ms,scenario,<metric>,...`.
- `<scenario>_<timestamp>.json` — the same samples plus run metadata.

---

## 中文

### 它是做什么的

`observer` 是一个 Minecraft 服务器的脚本化压测/观测包装器：接管服务器的 stdin/stdout，用一种叫 `.mcb` 的小脚本语言驱动服务器按剧本执行，并把采集到的指标导出为带时间戳的 CSV + JSON。

剧本可以做：

- **start** 用配置里的命令启动服务器。
- **wait** 等待服务器日志里出现指定模式（例如 `Done (`）。
- **send** 向服务器控制台发送命令。
- **grab** 从服务器日志里抓取数值，存为命名指标。
- **loop** 以固定间隔、在固定时长内重复一段步骤（典型用法：定期采样 MSPT / TPS）。
- **stop** 发送 `stop` 并在超时后强制 kill，防止孤儿进程。

每次运行都会在 `results_dir` 下生成以场景名 + 时间戳命名的 CSV 和 JSON。

两种使用方式：

- **TUI 模式**（默认）— 基于 ratatui 的完整界面：配置表单、场景选择、实时日志、进度面板。
- **无头模式** — `observer run ...` 跑完选定场景后退出，适合 CI。

### `.mcb` 脚本语言

```
# example_tps_stress.mcb
start                               # 启动服务器
wait   Done (                       # 等 Fabric/Paper 的就绪行
send   /tick rate 20
sleep  2s

loop 30s every 1s
    send    /tick query rate
    grab    mspt  Average tick time: {} ms
end

stop
```

指令：

| 指令 | 语法 | 说明 |
|------|------|------|
| `start` | `start` | 用 `config.server_cmd` 在 `config.server_dir` 启动服务器。 |
| `wait` | `wait [<超时>] <模式>` | 阻塞直到日志匹配。默认超时 `120s`。 |
| `send` | `send <命令>` | 向服务器 stdin 写入一行。 |
| `sleep` | `sleep <时长>` | 可中断的 sleep。 |
| `grab` | `grab <指标名> <模式>` | 从下一条匹配行抓一个数值写入 `<指标名>`。模式必须含 `{}` 或正则捕获组。 |
| `loop` | `loop <总时长> every <间隔>` … `end` | 在 `总时长` 内循环执行 body，每次迭代至少间隔 `间隔`。 |
| `stop` | `stop` | 发送 `stop`，最多等 `60s`，仍未退出则 kill。 |

模式语法：

- **字面量**（默认）— 子串匹配，正则特殊字符自动转义。`Done (` 就是字面匹配 `Done (`。
- **占位符** — `{}` 抓一个十进制数字（例 `Average: {} ms`），每个模式至多一个。
- **原始正则** — 以 `re:` 开头使用完整正则（例 `re:^\[.*\] Done \((\d+\.\d+)s\)`）。

时长单位：`ms` / `s` / `m`（只接受整数）。例 `500ms`、`2s`、`3m`。

注释：一行中 `#` 之后的内容忽略。

### 配置文件（`observer.json`）

```json
{
  "server_dir": "/path/to/server",
  "server_cmd": ["java", "-Xmx4G", "-Xms4G", "-jar", "server.jar", "nogui"],
  "lang": "zh",
  "scenarios_dir": "./scenarios",
  "selected_scenarios": ["example_tps_stress"],
  "results_dir": "./results"
}
```

- `server_cmd` 是启动服务器用的 argv。
- `selected_scenarios` 是 `observer run` 不带参数时的默认场景列表。
- `lang` 控制 TUI 语言（`en` / `zh`），不填则按 `$LANG` 自动判断。

### 用法

```
# TUI 模式（默认）
observer
observer tui [--config <路径>]

# 无头模式：执行场景，导出 CSV+JSON 后退出
observer run [--config <路径>] [场景名 ...]

# 脚本工具
observer s init   <名字>          # 在 scenarios_dir 下新建 <名字>.mcb
observer s format <名字或路径>    # 规整缩进和空白
observer s check  <名字或路径>    # 解析并报告语法错误
```

### 构建

需要支持 `edition = "2024"` 的 Rust（stable 1.85+）。

```sh
cargo build --release
./target/release/observer
```

### 输出

每次运行在 `results_dir` 下产出两个文件：

- `<场景>_<时间戳>.csv` — 每条 `grab` 采样一行，列为 `timestamp_ms,scenario,<指标>,...`。
- `<场景>_<时间戳>.json` — 同样的采样数据加上运行元信息。
