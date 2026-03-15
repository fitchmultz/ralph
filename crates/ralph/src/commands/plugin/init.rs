//! Plugin scaffold generation.
//!
//! Responsibilities:
//! - Validate `ralph plugin init` inputs.
//! - Build scaffold manifests and write optional runner/processor stubs.
//!
//! Not handled here:
//! - Plugin discovery/listing or uninstall workflows.
//! - CLI parsing or command dispatch.
//!
//! Invariants/assumptions:
//! - Empty capability flags default to scaffolding both runner and processor stubs.
//! - Relative `--path` values resolve from the repo root.
//! - Scaffolded scripts are marked executable on Unix platforms.

use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};

use crate::cli::plugin::PluginInitArgs;
use crate::commands::plugin::common::{print_enable_hint, scope_root};
use crate::commands::plugin::templates::{PROCESSOR_SCRIPT_TEMPLATE, RUNNER_SCRIPT_TEMPLATE};
use crate::config::Resolved;
use crate::plugins::PLUGIN_API_VERSION;
use crate::plugins::manifest::{PluginManifest, ProcessorPlugin, RunnerPlugin};

pub(super) fn run_init(resolved: &Resolved, args: &PluginInitArgs) -> Result<()> {
    validate_plugin_id(&args.id)?;

    let (with_runner, with_processor) = scaffold_capabilities(args);
    let target_dir = target_dir(resolved, args)?;

    if target_dir.exists() && !args.force {
        bail!(
            "Plugin directory already exists: {}. Use --force to overwrite.",
            target_dir.display()
        );
    }

    let manifest = build_manifest(args, with_runner, with_processor)?;
    let manifest_json = serde_json::to_string_pretty(&manifest)?;
    let runner_script =
        with_runner.then(|| RUNNER_SCRIPT_TEMPLATE.replace("{plugin_id}", &args.id));
    let processor_script =
        with_processor.then(|| PROCESSOR_SCRIPT_TEMPLATE.replace("{plugin_id}", &args.id));

    if args.dry_run {
        print_dry_run_plan(&target_dir, with_runner, with_processor);
        return Ok(());
    }

    fs::create_dir_all(&target_dir)
        .with_context(|| format!("create plugin directory {}", target_dir.display()))?;
    crate::fsutil::write_atomic(&target_dir.join("plugin.json"), manifest_json.as_bytes())
        .context("write plugin.json")?;

    write_script_if_present(target_dir.join("runner.sh"), runner_script.as_deref())
        .context("write runner.sh")?;
    write_script_if_present(target_dir.join("processor.sh"), processor_script.as_deref())
        .context("write processor.sh")?;

    println!("Created plugin {} at {}", args.id, target_dir.display());
    println!();
    println!("Files created:");
    println!("  plugin.json");
    if with_runner {
        println!("  runner.sh");
    }
    if with_processor {
        println!("  processor.sh");
    }
    print_enable_hint(&args.id);
    println!();
    println!("Validate the plugin:");
    println!("  ralph plugin validate --id {}", args.id);

    Ok(())
}

fn validate_plugin_id(id: &str) -> Result<()> {
    if id.contains('/') || id.contains('\\') {
        bail!("plugin id must not contain path separators");
    }
    if id.trim().is_empty() {
        bail!("plugin id must be non-empty");
    }
    Ok(())
}

fn scaffold_capabilities(args: &PluginInitArgs) -> (bool, bool) {
    let default_both = !args.with_runner && !args.with_processor;
    (
        args.with_runner || default_both,
        args.with_processor || default_both,
    )
}

fn target_dir(resolved: &Resolved, args: &PluginInitArgs) -> Result<PathBuf> {
    Ok(if let Some(path) = &args.path {
        if path.is_absolute() {
            path.clone()
        } else {
            resolved.repo_root.join(path)
        }
    } else {
        scope_root(&resolved.repo_root, args.scope)?.join(&args.id)
    })
}

fn build_manifest(
    args: &PluginInitArgs,
    with_runner: bool,
    with_processor: bool,
) -> Result<PluginManifest> {
    let manifest = PluginManifest {
        api_version: PLUGIN_API_VERSION,
        id: args.id.clone(),
        version: args.version.clone(),
        name: args
            .name
            .clone()
            .unwrap_or_else(|| default_name_from_id(&args.id)),
        description: args.description.clone(),
        runner: with_runner.then(|| RunnerPlugin {
            bin: "runner.sh".to_string(),
            supports_resume: Some(false),
            default_model: None,
        }),
        processors: with_processor.then(|| ProcessorPlugin {
            bin: "processor.sh".to_string(),
            hooks: vec![
                "validate_task".to_string(),
                "pre_prompt".to_string(),
                "post_run".to_string(),
            ],
        }),
    };
    manifest.validate().context("validate generated manifest")?;
    Ok(manifest)
}

fn default_name_from_id(id: &str) -> String {
    id.replace(['.', '-', '_'], " ")
}

fn print_dry_run_plan(target_dir: &Path, with_runner: bool, with_processor: bool) {
    println!("Would create plugin directory: {}", target_dir.display());
    println!("Would write: {}", target_dir.join("plugin.json").display());
    if with_runner {
        println!("Would write: {}", target_dir.join("runner.sh").display());
    }
    if with_processor {
        println!("Would write: {}", target_dir.join("processor.sh").display());
    }
}

fn write_script_if_present(path: PathBuf, contents: Option<&str>) -> Result<()> {
    let Some(contents) = contents else {
        return Ok(());
    };

    crate::fsutil::write_atomic(&path, contents.as_bytes())?;
    set_executable_permissions(&path)?;
    Ok(())
}

fn set_executable_permissions(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{default_name_from_id, scaffold_capabilities, validate_plugin_id};
    use crate::cli::plugin::{PluginInitArgs, PluginScopeArg};

    fn sample_args() -> PluginInitArgs {
        PluginInitArgs {
            id: "acme.super_runner".to_string(),
            scope: PluginScopeArg::Project,
            path: None,
            name: None,
            version: "0.1.0".to_string(),
            description: None,
            with_runner: false,
            with_processor: false,
            dry_run: false,
            force: false,
        }
    }

    #[test]
    fn default_name_from_id_replaces_common_separators() {
        assert_eq!(
            default_name_from_id("acme.super-runner_test"),
            "acme super runner test"
        );
    }

    #[test]
    fn validate_plugin_id_rejects_path_separators_and_blank_ids() {
        assert!(validate_plugin_id("foo/bar").is_err());
        assert!(validate_plugin_id("foo\\bar").is_err());
        assert!(validate_plugin_id("   ").is_err());
        assert!(validate_plugin_id("good.plugin").is_ok());
    }

    #[test]
    fn scaffold_capabilities_defaults_to_both_when_no_flags_are_set() {
        let args = sample_args();
        assert_eq!(scaffold_capabilities(&args), (true, true));
    }

    #[test]
    fn scaffold_capabilities_respects_explicit_runner_only_selection() {
        let mut args = sample_args();
        args.with_runner = true;
        assert_eq!(scaffold_capabilities(&args), (true, false));
    }
}
