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

use std::{env, fmt, time::UNIX_EPOCH};

use nu_ansi_term::{Color, Style};
use tracing::{field::Field, Event, Level as TracingLevel, Metadata, Subscriber};
use tracing_appender::non_blocking::NonBlocking;
use tracing_subscriber::{
    fmt::{
        format, format::FmtSpan, time::FormatTime, FmtContext, FormatEvent, FormatFields,
        FormattedFields, Layer as FmtLayer,
    },
    layer::{Context, Filter, SubscriberExt},
    registry::LookupSpan,
    util::SubscriberInitExt,
    Layer, Registry,
};

use crate::{util::time::DateTime, Result};

// Creates a `verbose` log level by wrapping an info! macro and
// adding a `verbose` field.
// This allows us to extract the field name in the event metadata and
// identify the event as a `verbose` event log.
// Currently, it only supports a subset of argument forms; additional arms
// can be added as needed to handle more use cases.
#[macro_export]
macro_rules! verbose {
    (target: $target:expr, $($arg:tt)*) => {
        tracing::info!(target: $target, verbose=true, $($arg)*);
    };
    ($($arg:tt)*) => {
        tracing::info!(verbose=true, $($arg)*);
    };
}

pub use verbose;

/// A custom log level type.
///
/// This extends the standard [`tracing::Level`] by introducing
/// additional log levels.
///
/// This type is intended for use in custom filtering, formatting,
/// and logging layers that need to distinguish custom logging levels.
#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub enum Level {
    Trace = 0,
    Debug = 1,
    Verbose = 2,
    Info = 3,
    Warn = 4,
    Error = 5,
}

impl Level {
    /// Creates a custom [`Level`] from a [`Metadata`] instance.
    ///
    /// This method inspects the metadata's fields for a custom level
    /// (e.g., a `verbose` field). If a custom level is present, it is returned.
    /// Otherwise, the standard `tracing::Level` is converted to this type.
    fn new(metadata: &Metadata) -> Self {
        if metadata.fields().field("verbose").is_some() {
            Level::Verbose
        } else {
            (*metadata.level()).into()
        }
    }
}

impl From<TracingLevel> for Level {
    fn from(value: TracingLevel) -> Self {
        match value {
            TracingLevel::TRACE => Level::Trace,
            TracingLevel::DEBUG => Level::Debug,
            TracingLevel::INFO => Level::Info,
            TracingLevel::WARN => Level::Warn,
            TracingLevel::ERROR => Level::Error,
        }
    }
}

impl fmt::Display for Level {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Level::Trace => write!(f, "TRACE"),
            Level::Debug => write!(f, "DEBUG"),
            Level::Verbose => write!(f, "VERBOSE"),
            Level::Info => write!(f, "INFO"),
            Level::Warn => write!(f, "WARN"),
            Level::Error => write!(f, "ERROR"),
        }
    }
}

/// A formatter for the custom [`Level`]
///
/// It implements [`fmt::Display`] and can produce a colored output
/// when writing to terminal and plain otherwise.
struct LevelFormatter<'a> {
    level: &'a Level,
    ansi: bool,
}

impl<'a> LevelFormatter<'a> {
    fn new(level: &'a Level, ansi: bool) -> Self {
        Self { level, ansi }
    }
}

impl fmt::Display for LevelFormatter<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.ansi {
            match self.level {
                Level::Trace => write!(f, "{}", Color::Purple.paint(format!("[{}]", self.level))),
                Level::Debug => write!(f, "{}", Color::Blue.paint(format!("[{}]", self.level))),
                Level::Verbose => write!(f, "{}", Color::Cyan.paint(format!("[{}]", self.level))),
                Level::Info => write!(f, "{}", Color::Green.paint(format!("[{}]", self.level))),
                Level::Warn => write!(f, "{}", Color::Yellow.paint(format!("[{}]", self.level))),
                Level::Error => write!(f, "{}", Color::Red.paint(format!("[{}]", self.level))),
            }
        } else {
            write!(f, "[{}]", self.level)
        }
    }
}

/// Formats event timestamps as `HH:MM:SS` for `tracing` output.
pub struct EventTimeFormatter;

impl FormatTime for EventTimeFormatter {
    fn format_time(&self, w: &mut format::Writer<'_>) -> fmt::Result {
        let now = DateTime::from_timestamp(UNIX_EPOCH.elapsed().unwrap().as_secs(), 0);
        write!(w, "{:02}:{:02}:{:02}", now.hour, now.min, now.sec)
    }
}

/// Formats `tracing` events for output, with support for custom log levels.
///
/// `EventFormatter` behaves like the default event formatter from
/// tracing-subscriber except for extracting custom levels from
/// event metadata fields and formatting them accordingly.
pub struct EventFormatter {
    ansi: bool,
    display_target: bool,
    timer: EventTimeFormatter,
}

impl EventFormatter {
    pub fn new(ansi: bool, display_target: bool) -> Self {
        Self { ansi, display_target, timer: EventTimeFormatter {} }
    }
}

impl<S, N> FormatEvent<S, N> for EventFormatter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: format::Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let meta = event.metadata();
        // creates a custom level
        let level = Level::new(meta);

        // format timestamp
        if self.ansi {
            let style = Style::new().dimmed();
            write!(writer, "{}", style.prefix())?;
            self.timer.format_time(&mut writer)?;
            write!(writer, "{} ", style.suffix())?
        } else {
            self.timer.format_time(&mut writer)?;
            write!(writer, " ")?;
        }

        // format custom level
        write!(writer, "{} ", LevelFormatter::new(&level, self.ansi))?;

        // format span
        let dimmed = if self.ansi { Style::new().dimmed() } else { Style::new() };

        if let Some(scope) = ctx.event_scope() {
            let bold = if self.ansi { Style::new().bold() } else { Style::new() };

            let mut seen = false;

            let spans: Vec<_> = scope.from_root().collect();
            let last_span_idx = spans.len() - 1;

            // Displays the full span tree
            for (span_idx, span) in spans.into_iter().enumerate() {
                let span_name = bold.paint(span.metadata().name());

                // Only need to show the span ID once for the root span
                // since its the same for all child spans too.
                if !seen {
                    // Crop span_id to 6 chars
                    let span_id = span.id().into_u64().to_string();
                    let span_id = &span_id[..span_id.len().min(6)];

                    write!(writer, "{}({})", span_name, span_id)?;
                } else {
                    write!(writer, "{}", span_name)?;
                }
                seen = true;

                // Only show the fields of the last span
                if meta.is_span() && span_idx == last_span_idx {
                    let ext = span.extensions();
                    if let Some(fields) = &ext.get::<FormattedFields<N>>() {
                        if !fields.is_empty() {
                            write!(writer, "{}{} {}", bold.paint("{"), fields, bold.paint("}"))?;
                        }
                    }
                }
                write!(writer, "{}", dimmed.paint(":"))?;
            }

            if seen {
                writer.write_char(' ')?;
            }
        };

        // format target
        if self.display_target {
            write!(writer, "{}{} ", dimmed.paint(meta.target()), dimmed.paint(":"))?;
        }

        // format event fields
        ctx.format_fields(writer.by_ref(), event)?;
        writeln!(writer)
    }
}

/// Formats event fields for terminal output, hiding level fields like `verbose`.
pub fn terminal_field_formatter(
    writer: &mut format::Writer<'_>,
    field: &Field,
    value: &dyn fmt::Debug,
) -> fmt::Result {
    match field.name() {
        // skip showing verbose field
        "verbose" => Ok(()),
        "message" => write!(writer, "{value:?}"),
        name if name.starts_with("log.") => Ok(()),
        name if name.starts_with("r#") => write!(
            writer,
            " {}{}{:?}",
            Style::new().italic().paint(&name[2..]),
            Style::new().dimmed().paint("="),
            value
        ),
        name => write!(
            writer,
            " {}{}{:?}",
            Style::new().italic().paint(name),
            Style::new().dimmed().paint("="),
            value
        ),
    }
}

/// Formats event fields for file logging, hiding level fields like `verbose`.
pub fn file_field_formatter(
    writer: &mut format::Writer<'_>,
    field: &Field,
    value: &dyn fmt::Debug,
) -> fmt::Result {
    match field.name() {
        // skip showing verbose field
        "verbose" => Ok(()),
        name if name.starts_with("log.") => Ok(()),
        name if name.starts_with("r#") => write!(writer, " {}={value:?}", &name[2..],),
        "message" => write!(writer, "{value:?}"),
        name => write!(writer, " {name}={value:?}"),
    }
}
/// A `tracing-subscriber` layer that filters events based on their target.
pub struct TargetFilter {
    /// Targets where logs should always be ignored.
    ignored_targets: Vec<String>,
    /// If non-empty, only these targets are allowed.
    allowed_targets: Vec<String>,
    /// Per-target minimum level that must be met for an event to be logged.
    target_levels: Vec<(String, Level)>,
    /// Default minimum level if no override is found
    default_level: Level,
}

impl TargetFilter {
    /// Create a `TargetFilter` by parsing `allowed_targets` and
    /// `ignored_targets` from a string in the format `target1,target2`
    /// or `!target1,!target2`
    pub fn parse_targets(mut self, log_targets: String) -> Self {
        let targets: Vec<String> = log_targets.split(',').map(|s| s.to_string()).collect();

        for target in targets {
            if target.starts_with('!') {
                self.ignored_targets.push(target.trim_start_matches('!').to_string());
                continue
            }

            self.allowed_targets.push(target.to_string());
        }

        self
    }

    /// Overrides default level with verbosity level.
    pub fn with_verbosity(mut self, verbosity_level: u8) -> Self {
        let level = match verbosity_level {
            0 => Level::Info,
            1 => Level::Verbose,
            2 => Level::Debug,
            _ => Level::Trace,
        };

        self.default_level = level;
        self
    }

    /// Adds multiple allowed targets.
    pub fn allow_targets<I, S>(mut self, targets: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.allowed_targets.extend(targets.into_iter().map(|s| s.as_ref().to_string()));
        self
    }

    /// Adds multiple ignored targets.
    pub fn ignore_targets<I, S>(mut self, targets: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.ignored_targets.extend(targets.into_iter().map(|s| s.as_ref().to_string()));
        self
    }

    /// Assign a log level filter for multiple targets.
    pub fn targets_level<I, S>(mut self, targets: I, level: Level) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.target_levels.extend(targets.into_iter().map(|s| (s.as_ref().to_string(), level)));
        self
    }

    /// Override default level filter.
    pub fn default_level(mut self, level: Level) -> Self {
        self.default_level = level;
        self
    }

    /// Helper method to implement the filtering logic.
    fn filter<S: Subscriber>(&self, metadata: &Metadata<'_>, _ctx: &Context<'_, S>) -> bool {
        let target = metadata.target();

        // Explicit ignore is given priority.
        if self.ignored_targets.iter().any(|s| target.starts_with(s)) {
            return false;
        }

        // If allowed list exists, only allow those.
        if !self.allowed_targets.is_empty() {
            return self.allowed_targets.iter().any(|s| target.starts_with(s));
        }

        let level = Level::new(metadata);
        // check level filter overrides.
        if let Some(min_level) = self
            .target_levels
            .iter()
            .filter(|(s, _)| target.starts_with(s))
            .max_by_key(|(s, _)| s.len()) // select the most specific target
            .map(|(_, lvl)| *lvl)
        {
            return level >= min_level;
        }

        // Otherwise default to level check.
        level >= self.default_level
    }
}

impl Default for TargetFilter {
    fn default() -> Self {
        Self {
            allowed_targets: Vec::new(),
            ignored_targets: Vec::new(),
            target_levels: Vec::new(),
            default_level: Level::Info,
        }
    }
}

/// Implement [`Filter`] to be able to filter inside a specific Layer.
impl<S: Subscriber> Filter<S> for TargetFilter {
    fn enabled(&self, metadata: &Metadata<'_>, ctx: &Context<'_, S>) -> bool {
        self.filter(metadata, ctx)
    }
}

/// Implement [`Filter`] to be able to add the filter as a Layer thus
/// filtering for multiple layers.
impl<S: Subscriber> Layer<S> for TargetFilter {
    fn enabled(&self, metadata: &Metadata<'_>, ctx: Context<'_, S>) -> bool {
        self.filter(metadata, &ctx)
    }
}

/// Helper for setting up logging for bins.
pub fn setup_logging(verbosity_level: u8, log_file: Option<NonBlocking>) -> Result<()> {
    let terminal_field_format = format::debug_fn(terminal_field_formatter);
    let file_field_format = format::debug_fn(file_field_formatter);

    let terminal_layer = FmtLayer::new()
        .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
        .event_format(EventFormatter::new(true, verbosity_level != 0))
        .fmt_fields(terminal_field_format)
        .with_writer(std::io::stdout);

    let mut target_filter = TargetFilter::default().with_verbosity(verbosity_level);
    if let Ok(log_targets) = env::var("LOG_TARGETS") {
        target_filter = target_filter.parse_targets(log_targets);
    }

    let file_layer = log_file.map(|log_file| {
        FmtLayer::new()
            .with_span_events(FmtSpan::NEW | FmtSpan::CLOSE)
            .event_format(EventFormatter::new(false, true))
            .fmt_fields(file_field_format)
            .with_writer(log_file)
    });

    Ok(Registry::default().with(terminal_layer).with(file_layer).with(target_filter).try_init()?)
}

/// Helper for setting up terminal logging for tests.
pub fn setup_test_logger(ignored_targets: &[&str], show_target: bool, level: Level) -> Result<()> {
    let terminal_field_format = format::debug_fn(terminal_field_formatter);
    let terminal_layer = FmtLayer::new()
        .event_format(EventFormatter::new(true, show_target))
        .fmt_fields(terminal_field_format)
        .with_writer(std::io::stdout);

    let target_filter =
        TargetFilter::default().ignore_targets(ignored_targets).default_level(level);

    Ok(Registry::default().with(terminal_layer).with(target_filter).try_init()?)
}
