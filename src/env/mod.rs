//! Environment, platform paths, and Java runtime discovery.

pub mod java;
pub mod paths;

pub use java::{JavaError, JavaInfo, extract_major_version, find_system_java_candidates, parse_java_version, probe_java_binary, required_java_major};
pub use paths::*;
