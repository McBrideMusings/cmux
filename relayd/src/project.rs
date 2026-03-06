use std::path::Path;

use crate::claude_detect::ClaudeDetector;
use crate::protocol::ProjectInfo;
use crate::session::SessionRegistry;
use uuid::Uuid;

/// Build a ProjectInfo for a given session.
pub fn build_project_info(
    session_id: Uuid,
    registry: &SessionRegistry,
    claude_detector: &ClaudeDetector,
) -> ProjectInfo {
    let sessions = registry.list();
    let session_info = sessions.iter().find(|s| s.id == session_id);

    let (cwd_str, state_str) = match session_info {
        Some(info) => (info.cwd.clone(), info.state.clone()),
        None => (String::new(), "unknown".to_string()),
    };

    let cwd = std::path::PathBuf::from(&cwd_str);
    let project_name = resolve_project_name(&cwd);
    let git_branch = resolve_git_branch(&cwd);
    let claude_code_detected = claude_detector.is_claude_detected_at(&cwd);

    ProjectInfo {
        session_id,
        project_name,
        git_branch,
        session_state: state_str,
        cwd: cwd_str,
        claude_code_detected,
    }
}

fn resolve_project_name(cwd: &Path) -> String {
    // Try package.json
    if let Some(name) = read_json_name(&cwd.join("package.json"), &["name"]) {
        return name;
    }

    // Try Cargo.toml
    if let Some(name) = read_toml_name(&cwd.join("Cargo.toml")) {
        return name;
    }

    // Try pyproject.toml
    if let Some(name) = read_toml_name(&cwd.join("pyproject.toml")) {
        return name;
    }

    // Fallback to directory basename
    cwd.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn read_json_name(path: &Path, keys: &[&str]) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let json: serde_json::Value = serde_json::from_str(&content).ok()?;
    let mut current = &json;
    for key in keys {
        current = current.get(*key)?;
    }
    current.as_str().map(|s| s.to_string())
}

fn read_toml_name(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    // Simple TOML parsing: look for name = "..." under [package] or [project]
    let mut in_section = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_section = trimmed == "[package]" || trimmed == "[project]";
            continue;
        }
        if in_section {
            if let Some(rest) = trimmed.strip_prefix("name") {
                let rest = rest.trim();
                if let Some(rest) = rest.strip_prefix('=') {
                    let rest = rest.trim();
                    if let Some(name) = rest.strip_prefix('"').and_then(|r| r.strip_suffix('"')) {
                        return Some(name.to_string());
                    }
                }
            }
        }
    }
    None
}

fn resolve_git_branch(cwd: &Path) -> Option<String> {
    // Try reading .git/HEAD directly
    let head_path = cwd.join(".git/HEAD");
    if let Ok(content) = std::fs::read_to_string(&head_path) {
        let content = content.trim();
        if let Some(refpath) = content.strip_prefix("ref: refs/heads/") {
            return Some(refpath.to_string());
        }
        // Detached HEAD — return short hash
        if content.len() >= 8 {
            return Some(content[..8].to_string());
        }
    }

    // Fallback: run git command
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(cwd)
        .output()
        .ok()?;

    if output.status.success() {
        let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !branch.is_empty() && branch != "HEAD" {
            return Some(branch);
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn resolve_project_name_fallback() {
        let name = resolve_project_name(&PathBuf::from("/tmp/my-cool-project"));
        assert_eq!(name, "my-cool-project");
    }

    #[test]
    fn parse_toml_name() {
        let dir = std::env::temp_dir().join("test-toml-parse");
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(
            dir.join("Cargo.toml"),
            "[package]\nname = \"my-crate\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let name = read_toml_name(&dir.join("Cargo.toml"));
        assert_eq!(name, Some("my-crate".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
