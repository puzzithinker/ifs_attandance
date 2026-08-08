use std::env;
use std::path::{Path, PathBuf};

/// Resolve DB path: CLI > exe_dir/agent.db > ./agent.db
pub fn resolve_db_path(cli: Option<&Path>) -> PathBuf {
    if let Some(p) = cli {
        return p.to_path_buf();
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            let p = dir.join("agent.db");
            // Prefer exe dir when it looks writable / is the install location
            if dir.exists() {
                return p;
            }
        }
    }
    PathBuf::from("agent.db")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_path_wins() {
        let p = Path::new("/data/event/agent.db");
        assert_eq!(resolve_db_path(Some(p)), PathBuf::from("/data/event/agent.db"));
    }

    #[test]
    fn without_cli_returns_some_agent_db_path() {
        let p = resolve_db_path(None);
        assert!(
            p.ends_with("agent.db"),
            "expected path ending in agent.db, got {}",
            p.display()
        );
    }
}
