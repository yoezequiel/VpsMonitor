//! Mirrors our own crates' log events into `shared::logbuf`, so the Agent
//! reporting loop and the Monitor's self-monitor loop can ship them
//! onward as `LogSource::Agent` / `LogSource::Monitor` entries (see
//! PLAN.md's log viewer). Lives here rather than in `shared` because only
//! `monitor-bin` depends on `tracing-subscriber`.

use shared::LogLevel;
use tracing_subscriber::Layer;

pub struct LogTapLayer;

impl<S> Layer<S> for LogTapLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        // Only our own code, not every dependency crate the env filter
        // happens to let through — vendored noise (hyper, sqlx query
        // logs...) isn't useful in the dashboard's log viewer.
        let target = event.metadata().target();
        if !(target.starts_with("monitor")
            || target.starts_with("agent")
            || target.starts_with("cli"))
        {
            return;
        }

        let level = match *event.metadata().level() {
            tracing::Level::TRACE => LogLevel::Trace,
            tracing::Level::DEBUG => LogLevel::Debug,
            tracing::Level::INFO => LogLevel::Info,
            tracing::Level::WARN => LogLevel::Warn,
            tracing::Level::ERROR => LogLevel::Error,
        };

        let mut message = String::new();
        event.record(&mut MessageVisitor(&mut message));
        if message.is_empty() {
            message = target.to_string();
        }

        shared::logbuf::push(level, message);
    }
}

struct MessageVisitor<'a>(&'a mut String);

impl tracing::field::Visit for MessageVisitor<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            use std::fmt::Write;
            let _ = write!(self.0, "{value:?}");
        }
    }
}
