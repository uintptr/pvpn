use std::process;

use crate::error::Result;

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

            out.finish(format_args!(
                "{}.{:03} :: [{:<5}] :: {:<5} :: {:<30} {}",
                now_sec,
                now_ms,
                process::id(),
                record.level(),
                record.target(),
                message
            ))
        })
        .level(log_level)
        .chain(std::io::stdout())
        .apply()?;
    Ok(())
}
