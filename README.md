# webmon

Polls a set of web pages on a schedule, diffs each one against its last
snapshot, and pushes a plain-text diff to [ntfy.sh](https://ntfy.sh) when
something changes.

- Config: a single TOML file.
- Per-target `include_regex` / `exclude_regex` (Rust `regex` syntax) to
  narrow down what counts as "changed" — or monitor the whole page by default.
- Two run modes: a self-looping **daemon**, or a **cron**-friendly single
  pass that only fetches targets whose interval has actually elapsed.
- State (last-checked time per target) is a plain-text, hand-editable file —
  no SQLite.
- Real logging via `log`/`fern`, to a file by default.

## Build

```
cargo build --release
# binary at target/release/webmon
```

## Configure

Copy `config.example.toml` to `config.toml` and edit it. See that file for
a fully-commented walkthrough of every option. Minimal example:

```toml
[general]
storage_dir = "./storage"
log_file = "./webmon.log"
state_file = "./webmon.state"
default_interval_secs = 3600

[[target]]
name = "example"
url = "https://example.com"
ntfy_topic = "my-ntfy-topic"
```

Relative paths in `[general]` are resolved against the config file's own
directory, not the current working directory — so it doesn't matter what
directory cron invokes webmon from.

## Run

**Daemon mode** — stays running, wakes up every `--tick-secs` (default 30)
and checks whether any target's own interval has elapsed:

```
./webmon --config config.toml daemon
./webmon --config config.toml daemon --tick-secs 10
```

**Cron mode** — does exactly one pass and exits; only fetches targets that
are actually due. Meant to be invoked periodically by an external
scheduler, e.g. once a minute:

```
./webmon --config config.toml cron
```

Example crontab entry:

```
* * * * * /path/to/webmon --config /path/to/config.toml cron
```

**Force a run now** (ignores schedules — handy for testing a new config or
regex before trusting it to the timer):

```
./webmon --config config.toml run-now
```

Add `-v` / `--verbose` to any of the above to also mirror log output to
stderr and bump the log level to debug (logging to the configured log file
always happens either way).

## Contributing

This project is meant to be understandable by someone writing code by hand.

It was bootstrapped with AI, however, to solve a pressing immediate need.
