//! One process-wide TRACE log capture shared by the secret-leak tests (N0042, N0045).
//! A global subscriber can be installed only once, so every test reads the same sink.
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub(crate) struct LogSink(pub(crate) Arc<Mutex<Vec<u8>>>);
impl std::io::Write for LogSink {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
/// Process-wide capture at TRACE, so zbus background threads are included too.
pub(crate) fn captured_logs() -> LogSink {
    static SINK: std::sync::OnceLock<LogSink> = std::sync::OnceLock::new();
    SINK.get_or_init(|| {
        let sink = LogSink::default();
        let writer = sink.clone();
        let _ = tracing::subscriber::set_global_default(
            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::TRACE)
                .with_ansi(false)
                .with_writer(move || writer.clone())
                .finish(),
        );
        sink
    })
    .clone()
}
