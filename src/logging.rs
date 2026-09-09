use anyhow::Result;
use std::path::Path;

/// Sets up logging to `log_file` (always) and, unless `quiet` is set, also
/// mirrors log lines to stderr. Default level is Info; `verbose` bumps it
/// to Debug.
pub fn init(log_file: &Path, verbose: bool, quiet: bool) -> Result<()> {
    let level = if verbose {
        log::LevelFilter::Debug
    } else {
        log::LevelFilter::Info
    };

    if let Some(parent) = log_file.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }

    let file_dispatch = fern::Dispatch::new()
        .format(|out, message, record| {
            out.finish(format_args!(
                "{} [{:<5}] {}: {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f"),
                record.level(),
                record.target(),
                message
            ))
        })
        .level(level)
        .chain(fern::log_file(log_file)?);

    let mut root = fern::Dispatch::new().chain(file_dispatch);

    if !quiet {
        let stderr_dispatch = fern::Dispatch::new()
            .format(|out, message, record| {
                out.finish(format_args!("[{:<5}] {}", record.level(), message))
            })
            .level(level)
            .chain(std::io::stderr());
        root = root.chain(stderr_dispatch);
    }

    root.apply()?;
    Ok(())
}
