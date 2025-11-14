/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2025 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer, Registry};
#[cfg(feature = "enable-filelog")]
use {
    file_rotate::{compression::Compression, suffix::AppendCount, ContentLimit, FileRotate},
    std::path::PathBuf,
};

#[cfg(target_os = "android")]
use tracing_subscriber::filter::{LevelFilter, Targets};

#[cfg(any(not(target_os = "android"), feature = "enable-filelog"))]
use {
    darkfi::util::logger::{EventFormatter, Level, TargetFilter},
    tracing_subscriber::fmt::format::FmtSpan,
};

// Measured in bytes
#[cfg(feature = "enable-filelog")]
const LOGFILE_MAXSIZE: usize = 5_000_000;

static MUTED_TARGETS: &[&'static str] = &[
    "sled",
    "rustls",
    "async_io",
    "polling",
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
#[cfg(not(target_os = "android"))]
static ALLOW_TRACE: &[&'static str] = &["ui", "app", "gfx"];

#[cfg(all(target_os = "android", feature = "enable-filelog"))]
fn logfile_path() -> PathBuf {
    use crate::android::get_external_storage_path;
    get_external_storage_path().join("darkfi-app.log")
}

#[cfg(all(not(target_os = "android"), feature = "enable-filelog"))]
fn logfile_path() -> PathBuf {
    dirs::cache_dir().unwrap().join("darkfi/darkfi-app.log")
}

pub fn setup_logging() -> Option<WorkerGuard> {
    let mut layers: Vec<(Box<dyn Layer<Registry> + Send + Sync>, Option<WorkerGuard>)> = vec![];

    #[cfg(feature = "enable-filelog")]
    {
        let (non_blocking_file_rotate, guard) = tracing_appender::non_blocking(FileRotate::new(
            logfile_path(),
            AppendCount::new(0),
            ContentLimit::BytesSurpassed(LOGFILE_MAXSIZE),
            Compression::None,
            #[cfg(unix)]
            None,
        ));

        let file_layer = tracing_subscriber::fmt::Layer::new()
            .event_format(EventFormatter::new(false, true))
            .fmt_fields(tracing_subscriber::fmt::format::debug_fn(
                darkfi::util::logger::file_field_formatter,
            ))
            .with_writer(non_blocking_file_rotate)
            .with_filter(
                TargetFilter::default()
                    .ignore_targets(["sled", "rustls", "async_io", "polling"])
                    .default_level(Level::Trace),
            );

        layers.push((file_layer.boxed(), Some(guard)));
    }

    #[cfg(target_os = "android")]
    {
        let logcat_layer =
            tracing_android::layer("darkfi").expect("tracing_android layer").with_filter(
                Targets::new()
                    .with_targets(
                        crate::logger::MUTED_TARGETS.iter().map(|&t| (t, LevelFilter::OFF)),
                    )
                    .with_default(LevelFilter::TRACE),
            );

        layers.push((logcat_layer.boxed(), None));
    }

    #[cfg(not(target_os = "android"))]
    {
        let mut terminal_layer = tracing_subscriber::fmt::Layer::new()
            .with_span_events(FmtSpan::ENTER | FmtSpan::CLOSE)
            .event_format(EventFormatter::new(true, true))
            .fmt_fields(tracing_subscriber::fmt::format::debug_fn(
                darkfi::util::logger::terminal_field_formatter,
            ))
            .with_writer(std::io::stdout)
            .with_filter(
                TargetFilter::default()
                    .targets_level(ALLOW_TRACE, Level::Trace)
                    .targets_level(MUTED_TARGETS, Level::Info)
                    .default_level(Level::Debug),
            );

        layers.push((terminal_layer.boxed(), None));
    }

    let file_logging_guard = layers.iter_mut().find_map(|l| l.1.take());

    Registry::default()
        .with(layers.into_iter().map(|l| l.0).collect::<Vec<_>>())
        .try_init()
        .expect("logger");

    file_logging_guard
}
