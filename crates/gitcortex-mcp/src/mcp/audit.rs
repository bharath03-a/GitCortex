//! Optional local call metadata. Never serializes user arguments or results.
use rmcp::model::CallToolResult;
use serde_json::json;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const MAX_BYTES: u64 = 1024 * 1024;

pub(crate) struct CallAudit {
    path: Option<PathBuf>,
    lock: Mutex<()>,
}

impl CallAudit {
    pub fn from_env() -> Self {
        Self {
            path: std::env::var_os("GCX_AUDIT_LOG")
                .filter(|p| !p.is_empty())
                .map(PathBuf::from),
            lock: Mutex::new(()),
        }
    }

    pub fn record(
        &self,
        action: &str,
        compact: bool,
        elapsed: Duration,
        result: &Result<CallToolResult, rmcp::ErrorData>,
    ) {
        if self.path.is_none() {
            return;
        }
        let (outcome, bytes, state, generation) = match result {
            Ok(result) => {
                let data = result.structured_content.as_ref();
                let state = data
                    .and_then(|v| v.get("index"))
                    .and_then(|v| v.get("state"))
                    .and_then(|v| v.as_str());
                // Whitelist states rather than logging arbitrary result strings.
                let state = state.filter(|s| {
                    matches!(
                        *s,
                        "ready" | "missing" | "stale" | "unavailable" | "unverified" | "degraded"
                    )
                });
                let status = data.and_then(|v| v.get("status")).and_then(|v| v.as_str());
                let outcome = match status {
                    Some("index_not_ready") => "index_not_ready",
                    Some("index_changed") => "index_changed",
                    _ if result.is_error == Some(true) => "tool_error",
                    _ => "ok",
                };
                let bytes = serde_json::to_vec(result).map(|v| v.len()).unwrap_or(0);
                let generation = data
                    .and_then(|v| v.get("graph_generation"))
                    .and_then(|v| v.as_u64());
                (outcome, bytes, state, generation)
            }
            Err(_) => ("protocol_error", 0, None, None),
        };
        let record = json!({"schema_version":1, "unix_ms":SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(),
            "process_id":std::process::id(), "action":action, "compact":compact,
            "duration_us":elapsed.as_micros(), "outcome":outcome, "response_bytes":bytes,
            "index_state":state, "graph_generation":generation});
        if let Err(error) = self.append(&record) {
            tracing::warn!("GCX audit write failed: {}", error.kind());
        }
    }

    fn append(&self, record: &serde_json::Value) -> std::io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let _guard = self
            .lock
            .lock()
            .map_err(|_| std::io::Error::other("audit mutex poisoned"))?;
        let mut line = serde_json::to_vec(record)?;
        line.push(b'\n');
        if line.len() as u64 > MAX_BYTES {
            return Err(std::io::Error::other("audit record exceeds file budget"));
        }
        let mut backup = path.as_os_str().to_owned();
        backup.push(".1");
        let backup = PathBuf::from(backup);
        // Repair files left by an older, larger retention policy.
        if fs::metadata(&backup).is_ok_and(|m| m.len() > MAX_BYTES) {
            fs::remove_file(&backup)?;
        }
        let current_bytes = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        if current_bytes > MAX_BYTES {
            OpenOptions::new().write(true).open(path)?.set_len(0)?;
        } else if current_bytes + line.len() as u64 > MAX_BYTES {
            if backup.exists() {
                fs::remove_file(&backup)?;
            }
            fs::rename(path, &backup)?;
        }
        let mut options = OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(path)?;
        file.write_all(&line)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn records_metadata_without_query_or_result_contents() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("calls.jsonl");
        let audit = CallAudit {
            path: Some(path.clone()),
            lock: Mutex::new(()),
        };
        audit.record("search_code", true, Duration::from_millis(2), &Ok(CallToolResult::structured(json!({"secret":"private source", "index":{"state":"stale"}, "status":"index_not_ready"}))));
        let text = fs::read_to_string(path).unwrap();
        assert!(!text.contains("private source"));
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["outcome"], "index_not_ready");
        assert_eq!(value["index_state"], "stale");
        assert_eq!(value["duration_us"], 2000);
    }
    #[test]
    fn repeated_rotation_never_exceeds_total_budget() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("calls.jsonl");
        let backup = temp.path().join("calls.jsonl.1");
        let audit = CallAudit {
            path: Some(path.clone()),
            lock: Mutex::new(()),
        };
        // Also exercise migration from the previous 8 MiB file budget.
        fs::File::create(&path)
            .unwrap()
            .set_len(MAX_BYTES * 8)
            .unwrap();
        fs::File::create(&backup)
            .unwrap()
            .set_len(MAX_BYTES * 8)
            .unwrap();
        audit.append(&json!({"action":"search_code"})).unwrap();
        assert!(!backup.exists());
        let record = json!({"padding":"x".repeat(1024)});
        for _ in 0..3200 {
            audit.append(&record).unwrap();
            let current = fs::metadata(&path).unwrap().len();
            let previous = fs::metadata(&backup).map(|m| m.len()).unwrap_or(0);
            assert!(current <= MAX_BYTES);
            assert!(previous <= MAX_BYTES);
            assert!(current + previous <= MAX_BYTES * 2);
        }
        assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 2);
    }

    #[test]
    fn rotates_and_preserves_protocol_failures() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("calls.jsonl");
        fs::File::create(&path).unwrap().set_len(MAX_BYTES).unwrap();
        let audit = CallAudit {
            path: Some(path.clone()),
            lock: Mutex::new(()),
        };
        audit.record(
            "unknown",
            false,
            Duration::ZERO,
            &Err(rmcp::ErrorData::invalid_params("sensitive", None)),
        );
        assert_eq!(
            fs::metadata(temp.path().join("calls.jsonl.1"))
                .unwrap()
                .len(),
            MAX_BYTES
        );
        let text = fs::read_to_string(path).unwrap();
        assert!(!text.contains("sensitive"));
        assert!(text.contains("protocol_error"));
    }
}
