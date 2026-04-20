# observer

Scripted benchmark runner and TUI wrapper for Minecraft servers. Drive a vanilla/Fabric/Paper server from the command line, run deterministic scenarios written in a tiny `.mcb` script language, and export timestamped metrics to CSV + JSON.

*[中文版 / Chinese](README.zh.md)*

## What it does

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

## `.mcb` scenario language

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

## Config (`observer.json`)

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

## Usage

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

## Build

Requires Rust with `edition = "2024"` support (stable 1.85+).

```sh
cargo build --release
./target/release/observer
```

## Output

Each run writes two files to `results_dir`:

- `<scenario>_<timestamp>.csv` — one row per `grab` sample, columns are `timestamp_ms,scenario,<metric>,...`.
- `<scenario>_<timestamp>.json` — the same samples plus run metadata.
