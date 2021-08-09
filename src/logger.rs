use anyhow::Result;
use flexi_logger::*;
use log::Level;
use std::fmt::Display;
use std::path::Path;

pub fn init_logger() -> Result<()> {
    let exe_path = &std::env::current_exe();
    let exe_dir = match exe_path {
        Ok(p) => p.parent(),
        Err(err) => {
            eprintln!("{}", err);
            None
        }
    };
    let log_dir = exe_dir.unwrap_or_else(|| Path::new("."));
    Logger::try_with_env_or_str("warn")?
        .log_to_file(FileSpec::default().directory(log_dir).suppress_timestamp())
        .adaptive_format_for_stdout(AdaptiveFormat::Opt)
        .adaptive_format_for_stderr(AdaptiveFormat::Opt)
        .format_for_files(opt_format)
        .start()?;

    Ok(())
}

pub trait ResultLogging {
    fn log(&self, level: Level);
    fn log_info(&self);
    fn log_err(&self);
}

impl<T, E> ResultLogging for Result<T, E>
where
    E: Display,
{
    fn log(&self, level: Level) {
        if let Err(err) = self {
            log::log!(level, "{}", err);
        }
    }

    fn log_info(&self) {
        self.log(Level::Info)
    }

    fn log_err(&self) {
        self.log(Level::Error)
    }
}
