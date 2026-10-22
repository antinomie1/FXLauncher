//! Minecraft crash report parsing and structured detail extraction.

use std::collections::HashMap;

/// Parsed structure of a standard `crash-YYYY-MM-DD_HH.MM.SS-client.txt` crash report.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CrashReport {
    pub description: String,
    pub time: Option<String>,
    pub suspected_mods: Vec<String>,
    pub system_details: HashMap<String, String>,
    pub stacktrace: String,
}

/// Parses the full text of a Minecraft crash report into a structured `CrashReport`.
pub fn parse_crash_report(content: &str) -> CrashReport {
    let mut report = CrashReport::default();

    let mut in_stacktrace = false;
    let mut in_system_details = false;
    let mut stacktrace_lines = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("Description:") {
            report.description = trimmed["Description:".len()..].trim().to_string();
            in_stacktrace = true;
            continue;
        }

        if trimmed.starts_with("Time:") {
            report.time = Some(trimmed["Time:".len()..].trim().to_string());
            continue;
        }

        if trimmed.starts_with("Suspected Mods:") || trimmed.starts_with("Suspected Mod:") {
            let mods_str = trimmed.split(':').nth(1).unwrap_or("");
            for part in mods_str.split(',') {
                let m = part.trim();
                if !m.is_empty() && !report.suspected_mods.contains(&m.to_string()) {
                    report.suspected_mods.push(m.to_string());
                }
            }
            continue;
        }

        if trimmed.starts_with("A detailed walkthrough of the error") {
            in_stacktrace = true;
            continue;
        }

        if trimmed.starts_with("-- System Details --") || trimmed.starts_with("System Details:") {
            in_stacktrace = false;
            in_system_details = true;
            continue;
        }

        if in_system_details {
            if let Some(colon_idx) = trimmed.find(':') {
                let key = trimmed[..colon_idx].trim().trim_start_matches('\t').to_string();
                let val = trimmed[colon_idx + 1..].trim().to_string();
                if !key.is_empty() {
                    report.system_details.insert(key, val);
                }
            }
        } else if in_stacktrace && !trimmed.is_empty() && !trimmed.starts_with("----") {
            stacktrace_lines.push(line.to_string());
        }
    }

    report.stacktrace = stacktrace_lines.join("\n");
    report
}
