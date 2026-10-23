//! Log line parsing and error log extraction.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Trace => write!(f, "TRACE"),
            Self::Debug => write!(f, "DEBUG"),
            Self::Info => write!(f, "INFO"),
            Self::Warn => write!(f, "WARN"),
            Self::Error => write!(f, "ERROR"),
            Self::Fatal => write!(f, "FATAL"),
        }
    }
}

/// A parsed structured log entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntry {
    pub timestamp: Option<String>,
    pub thread: Option<String>,
    pub level: LogLevel,
    pub message: String,
}

/// Parses a single log line (e.g. `[12:34:56] [main/INFO]: Message`).
pub fn parse_log_line(line: &str) -> Option<LogEntry> {
    let trimmed = line.trim();
    if !trimmed.starts_with('[') {
        return None;
    }

    // Find timestamp bracket: [12:34:56]
    let first_close = trimmed.find(']')?;
    let timestamp = trimmed[1..first_close].trim().to_string();

    let rest = trimmed[first_close + 1..].trim();
    if !rest.starts_with('[') {
        return None;
    }

    // Find thread and level bracket: [main/INFO] or [Render thread/ERROR]
    let second_close = rest.find(']')?;
    let thread_and_level = &rest[1..second_close];

    let message_start = rest[second_close + 1..].trim_start_matches(':').trim();

    let (thread, level) = if let Some(slash_idx) = thread_and_level.rfind('/') {
        let t = thread_and_level[..slash_idx].trim().to_string();
        let lvl_str = thread_and_level[slash_idx + 1..].trim().to_uppercase();
        let lvl = match lvl_str.as_str() {
            "FATAL" => LogLevel::Fatal,
            "ERROR" => LogLevel::Error,
            "WARN" | "WARNING" => LogLevel::Warn,
            "INFO" => LogLevel::Info,
            "DEBUG" => LogLevel::Debug,
            "TRACE" => LogLevel::Trace,
            _ => LogLevel::Info,
        };
        (Some(t), lvl)
    } else {
        (None, LogLevel::Info)
    };

    Some(LogEntry {
        timestamp: Some(timestamp),
        thread,
        level,
        message: message_start.to_string(),
    })
}

/// Extracts all lines with ERROR or FATAL levels from a multi-line log string.
pub fn extract_error_logs(content: &str) -> Vec<LogEntry> {
    let mut errors = Vec::new();

    for line in content.lines() {
        if let Some(entry) = parse_log_line(line)
            && (entry.level == LogLevel::Error || entry.level == LogLevel::Fatal) {
                errors.push(entry);
            }
    }

    errors
}
