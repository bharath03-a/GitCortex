use std::path::Path;

pub(crate) fn run_git_diff(repo_root: &Path, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(repo_root)
        .output()
        .ok()?;
    if out.status.success() {
        String::from_utf8(out.stdout).ok()
    } else {
        None
    }
}

/// Parse unified diff text into `(repo_relative_file_path, [(start_line, end_line)])`.
pub(crate) fn parse_diff_hunks(diff: &str) -> Vec<(String, Vec<(u32, u32)>)> {
    let mut result: Vec<(String, Vec<(u32, u32)>)> = Vec::new();
    let mut cur_file: Option<String> = None;
    let mut cur_hunks: Vec<(u32, u32)> = Vec::new();

    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("+++ b/") {
            if let Some(f) = cur_file.take() {
                if !cur_hunks.is_empty() {
                    result.push((f, std::mem::take(&mut cur_hunks)));
                }
            }
            cur_file = Some(path.to_owned());
        } else if line.starts_with("@@ ") {
            if let Some(hunk) = parse_hunk_header(line) {
                cur_hunks.push(hunk);
            }
        }
    }
    if let Some(f) = cur_file {
        if !cur_hunks.is_empty() {
            result.push((f, cur_hunks));
        }
    }
    result
}

/// Extract the new-file line range from a unified diff hunk header.
/// `@@ -old_start[,old_count] +new_start[,new_count] @@`
pub(crate) fn parse_hunk_header(line: &str) -> Option<(u32, u32)> {
    let rest = line.strip_prefix("@@ ")?;
    let plus_pos = rest.find(" +")?;
    let new_part = &rest[plus_pos + 2..];
    let end = new_part.find(' ').unwrap_or(new_part.len());
    let range = &new_part[..end];
    if let Some(comma) = range.find(',') {
        let start: u32 = range[..comma].parse().ok()?;
        let count: u32 = range[comma + 1..].parse().ok()?;
        if count == 0 {
            return None; // deletion-only hunk — no new lines to attribute
        }
        Some((start, start + count - 1))
    } else {
        let start: u32 = range.parse().ok()?;
        Some((start, start))
    }
}
