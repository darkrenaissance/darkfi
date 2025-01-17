use file_rotate::{compression::Compression, suffix::AppendCount, ContentLimit, FileRotate};
use log::{Level, LevelFilter, Log, Metadata, Record};
use simplelog::{
    ColorChoice, CombinedLogger, Config, ConfigBuilder, SharedLogger, TermLogger, TerminalMode,
    WriteLogger,
};
use std::{path::PathBuf, thread::sleep, time::Duration};

#[cfg(target_os = "android")]
const LOGS_ENABLED: bool = true;

#[cfg(not(target_os = "android"))]
const LOGS_ENABLED: bool = true;

// Measured in bytes
const LOGFILE_MAXSIZE: usize = 5_000_000;

static MUTED_TARGETS: &[&'static str] = &[
    "sled",
    "rustls",
    "net::channel",
    "net::message_publisher",
    "net::hosts",
    "net::protocol",
    "net::session",
    "net::outbound_session",
    "net::tcp",
    "net::p2p::seed",
    "net::refinery::handshake_node()",
    "system::publisher",
    "event_graph::dag_sync()",
    "event_graph::dag_insert()",
    "event_graph::protocol",
];

#[cfg(target_os = "android")]
fn logfile_path() -> PathBuf {
    use crate::android::get_external_storage_path;
    get_external_storage_path().join("Download/darkfi.log")
}

#[cfg(not(target_os = "android"))]
fn logfile_path() -> PathBuf {
    dirs::cache_dir().unwrap().join("darkfi/darkfi.log")
}

#[cfg(target_os = "android")]
mod android {
    use super::*;
    use android_logger::{AndroidLogger, Config as AndroidConfig};

    /// Implements a wrapper around the android logger so it's compatible with simplelog.
    pub struct AndroidLoggerWrapper {
        logger: AndroidLogger,
        level: LevelFilter,
        config: Config,
    }

    impl AndroidLoggerWrapper {
        pub fn new(level: LevelFilter, config: Config) -> Box<Self> {
            let cfg = AndroidConfig::default().with_max_level(level).with_tag("darkfi");
            Box::new(Self { logger: AndroidLogger::new(cfg), level, config })
        }
    }

    impl Log for AndroidLoggerWrapper {
        fn enabled(&self, metadata: &Metadata<'_>) -> bool {
            let target = metadata.target();
            for muted in MUTED_TARGETS {
                if target.starts_with(muted) {
                    return false
                }
            }
            if metadata.level() > self.level {
                return false
            }
            self.logger.enabled(metadata)
        }

        fn log(&self, record: &Record<'_>) {
            if self.enabled(record.metadata()) {
                self.logger.log(record)
            }
        }

        fn flush(&self) {}
    }

    impl SharedLogger for AndroidLoggerWrapper {
        fn level(&self) -> LevelFilter {
            self.level
        }

        fn config(&self) -> Option<&Config> {
            Some(&self.config)
        }

        fn as_log(self: Box<Self>) -> Box<dyn Log> {
            Box::new(*self)
        }
    }
}

pub fn setup_logging() {
    // https://gist.github.com/jb-alvarado/6e223936446bb88cd9a93e7028fc2c4f
    let mut loggers: Vec<Box<dyn SharedLogger>> = vec![];

    let mut cfg = ConfigBuilder::new();
    for target in MUTED_TARGETS {
        cfg.add_filter_ignore_str(target);
    }
    let cfg = cfg.build();

    if LOGS_ENABLED {
        let log_file = FileRotate::new(
            logfile_path(),
            AppendCount::new(0),
            ContentLimit::BytesSurpassed(LOGFILE_MAXSIZE),
            Compression::None,
            #[cfg(unix)]
            None,
        );
        let file_logger = WriteLogger::new(LevelFilter::Debug, cfg.clone(), log_file);
        loggers.push(file_logger);
    }

    #[cfg(target_os = "android")]
    {
        use android::AndroidLoggerWrapper;
        let android_logger = AndroidLoggerWrapper::new(LevelFilter::Debug, cfg);
        loggers.push(android_logger);
    }

    #[cfg(not(target_os = "android"))]
    {
        // For ANSI colors in the terminal
        colored::control::set_override(true);

        let term_logger =
            TermLogger::new(LevelFilter::Debug, cfg, TerminalMode::Mixed, ColorChoice::Auto);
        loggers.push(term_logger);
    }

    CombinedLogger::init(loggers).expect("logger");
}
