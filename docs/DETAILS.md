# Details


### Selecting what to monitor

- Neither `include_regex` nor `exclude_regex` set: the whole fetched page is
  compared byte-for-byte (after UTF-8 decoding).
- `include_regex` set: only the matched substrings are kept (concatenated,
  one per line, in the order they appear), and everything else is ignored.
  Use this to watch just a price, a status word, a table cell, etc.
- `exclude_regex` set: any matched substrings are stripped out before
  comparing. Use this to ignore timestamps, request IDs, ad slots, or other
  noise that changes on every load but isn't interesting.
- Both can be set together: `include_regex` is applied first, then
  `exclude_regex` is applied to what's left.
- Patterns are standard Rust `regex` syntax. Add inline flags inside the
  pattern if needed, e.g. `(?s)` for dot-matches-newline or `(?m)` for
  multiline `^`/`$` anchors (see `config.example.toml` for examples).

Regexes are validated at startup — a typo'd pattern fails fast with a clear
error instead of silently never matching.


## How it decides something "changed"

On the first fetch of a target there's nothing to compare against yet, so
webmon just saves a baseline snapshot and does not notify. From then on,
each run:

1. Fetches the URL.
2. Applies `include_regex` / `exclude_regex` to both the new content and the
   previously stored snapshot.
3. If the resulting text differs, generates a unified-style plain-text diff
   and POSTs it to the target's `ntfy_topic` (skipped if no topic is set —
   the target is still snapshotted and logged either way).
4. Overwrites the stored snapshot with the newly fetched raw page, so the
   next comparison is always against the most recent fetch.

## State and storage layout

Everything lives under `storage_dir`, which is created on demand:

- `storage_dir/webmon.log`: the log file.
- `storage_dir/webmon.state`: plain text, one `name<TAB>unix-timestamp` line
  per target. Delete a line (or the whole file) to force that target to be
  treated as never-checked; hand-edit a timestamp to change when it's next
  due.
- `storage_dir/pages/<name>.snapshot`: the raw last-fetched page for each
  target, one file per target.

## Notes

- HTTP requests are synchronous/blocking (one target fetched after another,
  within a single run). This keeps the implementation simple; for typical
  hourly-ish polling of a handful of pages this is not a bottleneck.
- ntfy.sh delivery is a plain HTTPS POST to `{ntfy_server}/{topic}` with the
  monitored URL on the first line followed by the diff as the request body,
  and the target name as the `Title` header —
  works with a self-hosted ntfy server too via `ntfy_server` /
  per-target `ntfy_server` override.

