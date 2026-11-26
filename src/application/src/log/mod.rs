//! Neutral, portable logging leaf. It has no Server target gate, HTTP layer,
//! product logic or online service requirement. Both Server and Client
//! products can consume the same event contract and implementation.

#[cfg(unix)]
mod rotating;
#[cfg(unix)]
pub use rotating::RotatingLogFile;

#[cfg(feature = "tracing")]
mod tracing_layer;
#[cfg(feature = "tracing")]
pub use tracing_layer::BlogStructuredLayer;

use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::io::{self, Write};

pub const MAX_RECORD_BYTES: usize = 16 * 1024;
pub const MAX_MESSAGE_BYTES: usize = 2048;
pub const MAX_IDENTIFIER_BYTES: usize = 128;

#[derive(Clone, Copy, Debug)]
pub struct LogRetention {
    /// Maximum bytes per active or archived file.
    pub file_bytes: u64,
    /// Between 1 and 32 archives, in addition to the active file.
    pub archives: u8,
}
impl Default for LogRetention {
    fn default() -> Self {
        Self {
            file_bytes: 8 * 1024 * 1024,
            archives: 4,
        }
    }
}

#[cfg(any(unix, windows))]
static PROCESS_SINK: std::sync::OnceLock<std::sync::Mutex<RotatingLogFile>> =
    std::sync::OnceLock::new();

/// Install one typed persistent sink after the application's state preflight.
/// A second installation is rejected, rather than silently replacing a writer.
/// # Errors
/// A process-wide log sink has already been installed.
#[cfg(any(unix, windows))]
pub fn install_rotating_file(sink: RotatingLogFile) -> Result<(), LogError> {
    PROCESS_SINK
        .set(std::sync::Mutex::new(sink))
        .map_err(|_| LogError::UnsafeStorage)
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Server,
    Instance,
}

/// Public templates are maintained here; products cannot override their
/// component, event, level or message while keeping a common event identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommonEvent {
    ConfigLoaded,
    InitializationCompleted,
    RuntimeStarted,
    ShutdownStarted,
    RuntimeStopped,
    DependencyUnavailable,
    TaskFailed,
    HttpAcceptFailed,
    HttpConnectionFailed,
    HttpConnectionClosed,
    MaintenanceStarted,
    MaintenanceCompleted,
}
impl CommonEvent {
    #[cfg(feature = "tracing")]
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|event| event.definition().1 == name)
    }
    #[cfg(any(test, feature = "tracing"))]
    const ALL: [Self; 12] = [
        Self::ConfigLoaded,
        Self::InitializationCompleted,
        Self::RuntimeStarted,
        Self::ShutdownStarted,
        Self::RuntimeStopped,
        Self::DependencyUnavailable,
        Self::TaskFailed,
        Self::HttpAcceptFailed,
        Self::HttpConnectionFailed,
        Self::HttpConnectionClosed,
        Self::MaintenanceStarted,
        Self::MaintenanceCompleted,
    ];
    fn definition(self) -> (&'static str, &'static str, Level, &'static str) {
        match self {
            Self::HttpAcceptFailed => (
                "http",
                "common.http.accept_failed",
                Level::Warn,
                "HTTP listener could not accept a connection.",
            ),
            Self::HttpConnectionFailed => (
                "http",
                "common.http.connection_failed",
                Level::Warn,
                "HTTP connection failed.",
            ),
            Self::HttpConnectionClosed => (
                "http",
                "common.http.connection_closed",
                Level::Info,
                "HTTP connection closed.",
            ),
            Self::ConfigLoaded => (
                "config",
                "common.config.loaded",
                Level::Info,
                "Current configuration loaded.",
            ),
            Self::InitializationCompleted => (
                "initialization",
                "common.initialization.completed",
                Level::Info,
                "First initialization completed.",
            ),
            Self::RuntimeStarted => (
                "runtime",
                "common.runtime.started",
                Level::Info,
                "Runtime started.",
            ),
            Self::ShutdownStarted => (
                "runtime",
                "common.runtime.shutdown_started",
                Level::Info,
                "Controlled shutdown started.",
            ),
            Self::RuntimeStopped => (
                "runtime",
                "common.runtime.stopped",
                Level::Info,
                "Runtime stopped.",
            ),
            Self::DependencyUnavailable => (
                "dependency",
                "common.dependency.unavailable",
                Level::Warn,
                "A required dependency is unavailable.",
            ),
            Self::TaskFailed => (
                "task",
                "common.task.failed",
                Level::Error,
                "A supervised task failed.",
            ),
            Self::MaintenanceStarted => (
                "maintenance",
                "common.maintenance.started",
                Level::Info,
                "Maintenance started.",
            ),
            Self::MaintenanceCompleted => (
                "maintenance",
                "common.maintenance.completed",
                Level::Info,
                "Maintenance completed.",
            ),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct LogRecord {
    timestamp: String,
    level: Level,
    service: String,
    component: String,
    event: String,
    message: String,
    scope: Scope,
    #[serde(skip_serializing_if = "Option::is_none")]
    instance_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    instance_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_id: Option<crate::error::RequestId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_code: Option<crate::error::ErrorCode>,
    #[serde(skip_serializing_if = "Map::is_empty")]
    attributes: Map<String, Value>,
}

impl LogRecord {
    #[cfg(feature = "tracing")]
    pub(crate) fn common_instance(
        service: &str,
        event: CommonEvent,
        instance_id: &str,
    ) -> Result<Self, LogError> {
        if !matches!(
            event,
            CommonEvent::DependencyUnavailable | CommonEvent::TaskFailed
        ) {
            return Err(LogError::InvalidRecord);
        }
        identifier(instance_id)?;
        let (component, name, level, message) = event.definition();
        Self::build(
            service,
            component,
            name,
            message,
            level,
            Some(instance_id.into()),
        )
    }
    /// # Errors
    /// The service name or resulting common event record is invalid.
    pub fn common(service: &str, event: CommonEvent) -> Result<Self, LogError> {
        let (component, event, level, message) = event.definition();
        Self::build(service, component, event, message, level, None)
    }
    /// # Errors
    /// The service, event, or message violates the log record limits.
    pub fn server(
        service: &str,
        component: &str,
        event: &str,
        message: &str,
        level: Level,
    ) -> Result<Self, LogError> {
        product_event(service, event)?;
        Self::build(service, component, event, message, level, None)
    }
    /// # Errors
    /// The service, instance, event, or message violates the log record limits.
    pub fn instance(
        service: &str,
        component: &str,
        event: &str,
        message: &str,
        level: Level,
        instance_id: &str,
    ) -> Result<Self, LogError> {
        product_event(service, event)?;
        identifier(instance_id)?;
        Self::build(
            service,
            component,
            event,
            message,
            level,
            Some(instance_id.into()),
        )
    }
    fn build(
        service: &str,
        component: &str,
        event: &str,
        message: &str,
        level: Level,
        instance_id: Option<String>,
    ) -> Result<Self, LogError> {
        identifier(service)?;
        identifier(component)?;
        identifier(event)?;
        if message.len() > MAX_MESSAGE_BYTES || message.chars().any(char::is_control) {
            return Err(LogError::InvalidRecord);
        }
        Ok(Self {
            timestamp: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            level,
            service: service.into(),
            component: component.into(),
            event: event.into(),
            message: sanitize_text(message),
            scope: if instance_id.is_some() {
                Scope::Instance
            } else {
                Scope::Server
            },
            instance_id,
            instance_type: None,
            request_id: None,
            task_id: None,
            error_code: None,
            attributes: Map::new(),
        })
    }
    #[cfg(feature = "tracing")]
    pub(crate) fn with_instance_type(mut self, instance_type: &str) -> Result<Self, LogError> {
        if self.scope != Scope::Instance {
            return Err(LogError::InvalidRecord);
        }
        identifier(instance_type)?;
        self.instance_type = Some(instance_type.into());
        Ok(self)
    }
    /// # Errors
    /// The request identifier violates its correlation-id constraints.
    pub fn with_request_id(mut self, request_id: &str) -> Result<Self, LogError> {
        self.request_id =
            Some(crate::error::RequestId::new(request_id).map_err(LogError::RequestId)?);
        Ok(self)
    }
    #[cfg(any(test, feature = "tracing"))]
    pub(crate) fn with_task_id(mut self, task_id: &str) -> Result<Self, LogError> {
        identifier(task_id)?;
        self.task_id = Some(task_id.into());
        Ok(self)
    }
    #[cfg(any(test, feature = "tracing"))]
    pub(crate) fn with_error_code(mut self, code: &str) -> Result<Self, LogError> {
        self.error_code = Some(crate::error::ErrorCode::new(code).map_err(LogError::ErrorCode)?);
        Ok(self)
    }
    pub(crate) fn with_attribute(
        mut self,
        key: &str,
        value: impl Into<Value>,
    ) -> Result<Self, LogError> {
        identifier(key)?;
        let mut attribute = json!({key: value.into()});
        redact(&mut attribute);
        let Value::Object(attribute) = attribute else {
            unreachable!("invariant: an attribute is constructed as a JSON object");
        };
        self.attributes.extend(attribute);
        self.json_line()?;
        Ok(self)
    }
    #[must_use]
    pub fn timestamp(&self) -> &str {
        &self.timestamp
    }
    #[must_use]
    pub fn level(&self) -> Level {
        self.level
    }
    #[must_use]
    pub fn scope(&self) -> Scope {
        self.scope
    }
    #[must_use]
    pub fn instance_id(&self) -> Option<&str> {
        self.instance_id.as_deref()
    }
    #[must_use]
    pub fn event(&self) -> &str {
        &self.event
    }
    pub(crate) fn json_line(&self) -> Result<Vec<u8>, LogError> {
        self.encode().map(|line| line.0)
    }

    fn encode(&self) -> Result<LogLine, LogError> {
        let mut limited = LimitedBuffer { bytes: Vec::new() };
        serde_json::to_writer(&mut limited, self).map_err(|error| {
            if error.is_io() {
                LogError::RecordTooLarge
            } else {
                unreachable!("invariant: log records contain only JSON-serializable fields");
            }
        })?;
        if limited.bytes.len() >= MAX_RECORD_BYTES {
            return Err(LogError::RecordTooLarge);
        }
        limited.bytes.push(b'\n');
        Ok(LogLine(limited.bytes))
    }
    /// # Errors
    /// The record cannot be encoded or written to the supplied writer.
    pub fn write_to(&self, output: &mut impl Write) -> Result<(), LogError> {
        write_line_to(&self.encode()?, output)
    }
    /// Stderr is a stream sink. The service manager owns stream capture,
    /// retention and rotation; no application-owned file grows behind it.
    pub(crate) fn emit_stderr(&self) -> Result<(), LogError> {
        let line = self.encode()?;
        write_line_to(&line, &mut io::stderr().lock())
    }
    /// Emit to the explicitly installed process sink, or to stderr when absent.
    /// Native background services install their persistent sink before work.
    /// # Errors
    /// The record cannot be encoded or written to the selected sink.
    pub fn emit(&self) -> Result<(), LogError> {
        emit_line(&self.encode()?)
    }
}

struct LogLine(Vec<u8>);

fn emit_line(line: &LogLine) -> Result<(), LogError> {
    #[cfg(any(unix, windows))]
    if let Some(sink) = PROCESS_SINK.get() {
        return sink
            .lock()
            .expect("process log sink lock poisoned")
            .write_line(line);
    }
    write_line_to(line, &mut io::stderr().lock())
}

fn write_line_to(line: &LogLine, output: &mut (impl Write + ?Sized)) -> Result<(), LogError> {
    output.write_all(&line.0)?;
    output.flush()?;
    Ok(())
}

struct LimitedBuffer {
    bytes: Vec<u8>,
}
impl Write for LimitedBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.bytes.len().saturating_add(bytes.len()) >= MAX_RECORD_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "log record exceeds its byte budget",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn identifier(value: &str) -> Result<(), LogError> {
    if value.is_empty()
        || value.len() > MAX_IDENTIFIER_BYTES
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b':' | b'-'))
    {
        Err(LogError::InvalidRecord)
    } else {
        Ok(())
    }
}
fn product_event(service: &str, event: &str) -> Result<(), LogError> {
    if event
        .strip_prefix(service)
        .is_none_or(|suffix| !suffix.starts_with('.') || suffix.len() <= 1)
    {
        Err(LogError::InvalidRecord)
    } else {
        Ok(())
    }
}

/// Field names, including mixed case and separators, identify secrets. Never
/// put credentials in a non-secret field or an arbitrary internal error chain.
pub fn redact(value: &mut Value) {
    match value {
        Value::Object(fields) => {
            for (key, value) in fields {
                let normalized: String = key
                    .chars()
                    .filter(char::is_ascii_alphanumeric)
                    .map(|c| c.to_ascii_lowercase())
                    .collect();
                if [
                    "password",
                    "token",
                    "secret",
                    "credential",
                    "privatekey",
                    "apikey",
                    "certificate",
                    "authorization",
                ]
                .iter()
                .any(|secret| normalized.contains(secret))
                {
                    if !value.is_boolean() {
                        let configured = match &*value {
                            Value::Null => false,
                            Value::String(value) => !value.is_empty(),
                            Value::Array(value) => !value.is_empty(),
                            Value::Object(value) => value
                                .get("configured")
                                .and_then(Value::as_bool)
                                .filter(|_| value.len() == 1)
                                .unwrap_or(!value.is_empty()),
                            Value::Bool(_) | Value::Number(_) => true,
                        };
                        *value = json!({"configured":configured});
                    }
                } else {
                    redact(value);
                }
            }
        }
        Value::Array(values) => {
            for value in values {
                redact(value);
            }
        }
        Value::String(value) => *value = sanitize_text(value),
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn sanitize_text(value: &str) -> String {
    let lower = value.to_ascii_lowercase();
    if (value.contains("://") && value.contains('@'))
        || [
            "password=",
            "password:",
            "token=",
            "token:",
            "authorization:",
            "bearer ",
            "-----begin private key",
        ]
        .iter()
        .any(|pattern| lower.contains(pattern))
    {
        "[sensitive text redacted]".into()
    } else {
        value.chars().filter(|c| !c.is_control()).collect()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LogError {
    #[error("structured log record does not satisfy the current contract")]
    InvalidRecord,
    #[error("structured log request ID is invalid")]
    RequestId(#[source] crate::error::InvalidRequestId),
    #[error("structured log error code is invalid")]
    ErrorCode(#[source] crate::error::InvalidErrorCode),
    #[error("structured log archive index is invalid")]
    ArchiveIndex(#[source] std::num::ParseIntError),
    #[error("structured log record exceeds its byte budget")]
    RecordTooLarge,
    #[error("structured log limits are invalid")]
    InvalidLimits,
    #[error("log storage must be a private directory with private regular files and one writer")]
    UnsafeStorage,
    #[error("failed to acquire the exclusive log writer lease")]
    WriterLock(#[source] io::Error),
    #[error("failed to release the exclusive log writer lease")]
    WriterRelease(#[source] io::Error),
    #[error("failed to append a log record and restore the previous length; rollback: {rollback}")]
    AppendRollback {
        #[source]
        source: io::Error,
        rollback: io::Error,
    },
    #[error("log writer is unavailable after a failed append rollback")]
    WriterFailed,
    #[error("log I/O failed")]
    Io(#[from] io::Error),
}

#[path = "../../../../test/application/unit/log/tests.rs"]
#[cfg(test)]
mod tests;
