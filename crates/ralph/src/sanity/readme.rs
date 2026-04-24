//! README auto-update logic for sanity checks.
//!
//! Purpose:
//! - README auto-update logic for sanity checks.
//!
//! Responsibilities:
//! - Check if README.md is outdated compared to embedded template
//! - Auto-update README without prompting (automatic operation)
//!
//! Not handled here:
//! - User prompts (automatic operation only)
//! - Migration handling (see migrations.rs)
//! - Unknown key detection (see unknown_keys.rs)
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants:
//! - README auto-update is always automatic, never prompts user
//! - Missing README is auto-created from the embedded template

use crate::config::Resolved;
use anyhow::{Context, Result};

/// Check and auto-update README if needed.
///
/// Returns `Ok(Some(message))` if README was updated.
/// Returns `Ok(None)` if README is current or not applicable.
pub(crate) fn check_and_update_readme(resolved: &Resolved) -> Result<Option<String>> {
    use crate::commands::init::readme;

    match readme::check_readme_current(resolved)? {
        readme::ReadmeCheckResult::Current(version) => {
            log::debug!("README is current (version {})", version);
            Ok(None)
        }
        readme::ReadmeCheckResult::Outdated {
            current_version,
            embedded_version,
        } => {
            let readme_path = resolved.repo_root.join(".ralph/README.md");
            log::info!(
                "README is outdated (version {} < {}), updating...",
                current_version,
                embedded_version
            );

            let (status, _) =
                readme::write_readme(&readme_path, false, true).context("write updated README")?;

            if status == crate::commands::init::FileInitStatus::Updated {
                let msg = format!(
                    "Updated README from version {} to {}",
                    current_version, embedded_version
                );
                log::info!("{}", msg);
                Ok(Some(msg))
            } else {
                log::debug!("README write returned status: {:?}", status);
                Ok(None)
            }
        }
        readme::ReadmeCheckResult::Missing => {
            let readme_path = resolved.repo_root.join(".ralph/README.md");
            log::info!("README is missing, creating {}", readme_path.display());
            let (status, version) =
                readme::write_readme(&readme_path, false, true).context("create missing README")?;
            if matches!(status, crate::commands::init::FileInitStatus::Created) {
                let version_display = version
                    .map(|v| v.to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                let msg = format!("Created README at version {}", version_display);
                log::info!("{}", msg);
                Ok(Some(msg))
            } else {
                log::debug!("README create write returned status: {:?}", status);
                Ok(None)
            }
        }
        readme::ReadmeCheckResult::NotApplicable => {
            log::debug!("README.md is not applicable");
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::init::check_readme_current;
    use crate::config;
    use crate::constants::versions::README_VERSION;
    use crate::contracts::Config;
    use tempfile::TempDir;

    fn resolved_for(dir: &TempDir) -> config::Resolved {
        let repo_root = dir.path().to_path_buf();
        let queue_path = repo_root.join(".ralph/queue.jsonc");
        let done_path = repo_root.join(".ralph/done.jsonc");
        let project_config_path = Some(repo_root.join(".ralph/config.jsonc"));
        config::Resolved {
            config: Config::default(),
            repo_root,
            queue_path,
            done_path,
            id_prefix: "RQ".to_string(),
            id_width: 4,
            global_config_path: None,
            project_config_path,
        }
    }

    #[test]
    fn check_and_update_readme_creates_missing_file() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = resolved_for(&dir);

        let fix = check_and_update_readme(&resolved)?;
        assert!(
            fix.as_deref()
                .is_some_and(|msg| msg.contains("Created README at version")),
            "expected create message, got: {:?}",
            fix
        );

        let readme_path = resolved.repo_root.join(".ralph/README.md");
        assert!(readme_path.exists(), "README should be created");
        let check = check_readme_current(&resolved)?;
        assert!(matches!(
            check,
            crate::commands::init::ReadmeCheckResult::Current(v) if v == README_VERSION
        ));
        Ok(())
    }

    #[test]
    fn check_and_update_readme_updates_outdated_file() -> Result<()> {
        let dir = TempDir::new()?;
        let resolved = resolved_for(&dir);
        let readme_path = resolved.repo_root.join(".ralph/README.md");
        std::fs::create_dir_all(readme_path.parent().expect("parent"))?;
        std::fs::write(&readme_path, "<!-- RALPH_README_VERSION: 1 -->\n# Old")?;

        let fix = check_and_update_readme(&resolved)?;
        assert!(
            fix.as_deref()
                .is_some_and(|msg| msg.contains("Updated README from version 1")),
            "expected update message, got: {:?}",
            fix
        );

        let check = check_readme_current(&resolved)?;
        assert!(matches!(
            check,
            crate::commands::init::ReadmeCheckResult::Current(v) if v == README_VERSION
        ));
        Ok(())
    }
}
