use std::path::Path;

use tracing_subscriber::{
    fmt,
    layer::SubscriberExt as _,
    util::SubscriberInitExt as _,
    EnvFilter,
};

/// Initialize a shared tracing subscriber for memory-api transport binaries.
///
/// Level resolution order:
/// 1. explicit `log_level`
/// 2. `RUST_LOG`
/// 3. `default_level` plus `service_directive`
pub fn init_transport_tracing(
    service_directive: &str,
    log_level: Option<&str>,
    log_file: Option<&Path>,
    default_level: &str,
) {
    let filter = if let Some(level) = log_level {
        EnvFilter::new(level)
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| {
            EnvFilter::new(default_level).add_directive(
                service_directive
                    .parse()
                    .expect("valid transport tracing directive"),
            )
        })
    };

    let stderr_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_ansi(true)
        .with_target(true)
        .with_thread_ids(false);

    if let Some(path) = log_file {
        let dir = path.parent().unwrap_or(Path::new("."));
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "transport.log".to_string());

        let file_appender = tracing_appender::rolling::never(dir, &file_name);
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
        std::mem::forget(guard);

        let file_layer = fmt::layer()
            .with_writer(non_blocking)
            .with_ansi(false)
            .with_target(true)
            .with_thread_ids(true);

        tracing_subscriber::registry()
            .with(filter)
            .with(stderr_layer)
            .with(file_layer)
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(stderr_layer)
            .init();
    }
}