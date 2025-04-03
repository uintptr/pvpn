use std::fmt::Display;

use log::info;

use crate::error::Result;

pub fn display_error<E>(err: E)
where
    E: std::error::Error,
{
    let mut source = err.source();

    while let Some(s) = source {
        info!("Caused by: {}", s);
        source = s.source();
    }
}

pub fn printkv<D>(k: &str, v: D)
where
    D: Display,
{
    let key = format!("{k}:");
    println!("    {key:<20}{v}");
}

pub fn setup_logger() -> Result<()> {
    let log_level = match cfg!(debug_assertions) {
        true => log::LevelFilter::Info,
        false => log::LevelFilter::Warn,
    };

    fern::Dispatch::new()
        .format(|out, message, record| {
            let now_ms = chrono::Local::now().timestamp_millis();
            let now_sec = now_ms / 1000;
            let now_ms = now_ms - (now_sec * 1000);

            let target = match record.line() {
                Some(v) => format!("{}:{v}", record.target()),
                None => record.target().to_string(),
            };

            out.finish(format_args!(
                "{}.{:03} :: {:<5} :: {:<30} {}",
                now_sec,
                now_ms,
                record.level(),
                target,
                message
            ))
        })
        .level(log_level)
        .chain(std::io::stdout())
        .apply()?;
    Ok(())
}
