//! Plugin command implementations.
//!
//! Responsibilities:
//! - Implement plugin list, validate, install, uninstall, and init commands.
//!
//! Not handled here:
//! - CLI argument parsing (see `crate::cli::plugin`).
//! - Plugin discovery/registry (see `crate::plugins`).

use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::cli::plugin::{PluginArgs, PluginCommand, PluginInitArgs, PluginScopeArg};
use crate::config::Resolved;
use crate::plugins::PLUGIN_API_VERSION;
use crate::plugins::discovery::{PluginScope, discover_plugins, plugin_roots};
use crate::plugins::manifest::{PluginManifest, ProcessorPlugin, RunnerPlugin};
use crate::plugins::registry::{PluginRegistry, resolve_plugin_relative_exe};

pub fn run(args: &PluginArgs, resolved: &Resolved) -> Result<()> {
    match &args.command {
        PluginCommand::List { json } => cmd_list(resolved, *json),
        PluginCommand::Validate { id } => cmd_validate(resolved, id.as_deref()),
        PluginCommand::Install { source, scope } => cmd_install(resolved, source, *scope),
        PluginCommand::Uninstall { id, scope } => cmd_uninstall(resolved, id, *scope),
        PluginCommand::Init(init_args) => cmd_init(resolved, init_args),
    }
}

fn cmd_list(resolved: &Resolved, json_output: bool) -> Result<()> {
    let registry = PluginRegistry::load(&resolved.repo_root, &resolved.config)?;
    let discovered = registry.discovered();

    if json_output {
        let output: BTreeMap<String, serde_json::Value> = discovered
            .iter()
            .map(|(id, d)| {
                let enabled = registry.is_enabled(id);
                let info = serde_json::json!({
                    "id": id,
                    "name": d.manifest.name,
                    "version": d.manifest.version,
                    "scope": match d.scope {
                        PluginScope::Global => "global",
                        PluginScope::Project => "project",
                    },
                    "enabled": enabled,
                    "has_runner": d.manifest.runner.is_some(),
                    "has_processors": d.manifest.processors.is_some(),
                });
                (id.clone(), info)
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        if discovered.is_empty() {
            println!("No plugins discovered.");
            println!();
            println!("Plugin directories checked:");
            for (scope, root) in plugin_roots(&resolved.repo_root) {
                let scope_str = match scope {
                    PluginScope::Global => "global",
                    PluginScope::Project => "project",
                };
                println!("  [{}] {}", scope_str, root.display());
            }
            return Ok(());
        }

        println!("Discovered plugins:");
        println!();
        for (id, d) in discovered.iter() {
            let enabled = registry.is_enabled(id);
            let scope_str = match d.scope {
                PluginScope::Global => "global",
                PluginScope::Project => "project",
            };
            let status = if enabled { "enabled" } else { "disabled" };
            let capabilities = {
                let mut caps = Vec::new();
                if d.manifest.runner.is_some() {
                    caps.push("runner");
                }
                if d.manifest.processors.is_some() {
                    caps.push("processors");
                }
                if caps.is_empty() {
                    "none".to_string()
                } else {
                    caps.join(", ")
                }
            };

            println!("  {} ({})", id, d.manifest.version);
            println!("    Name:    {}", d.manifest.name);
            println!("    Scope:   {}", scope_str);
            println!("    Status:  {}", status);
            println!("    Capabilities: {}", capabilities);
            if let Some(desc) = &d.manifest.description {
                println!("    Description: {}", desc);
            }
            println!();
        }

        println!("To enable a plugin, add to your config:");
        println!(
            r#"  {{ "plugins": {{ "plugins": {{ "<plugin-id>": {{ "enabled": true }} }} }} }}"#
        );
    }

    Ok(())
}

fn cmd_validate(resolved: &Resolved, filter_id: Option<&str>) -> Result<()> {
    let discovered = discover_plugins(&resolved.repo_root)?;

    if discovered.is_empty() {
        println!("No plugins to validate.");
        return Ok(());
    }

    let mut validated = 0;
    let mut errors = 0;

    for (id, d) in discovered.iter() {
        if let Some(filter) = filter_id
            && id != filter
        {
            continue;
        }

        print!("Validating {}... ", id);

        // Manifest was already validated during discovery, but re-validate for thoroughness
        if let Err(e) = d.manifest.validate() {
            println!("FAILED (manifest)");
            println!("  Error: {}", e);
            errors += 1;
            continue;
        }

        // Check runner binary exists if specified
        if let Some(runner) = &d.manifest.runner {
            let bin_path = resolve_plugin_relative_exe(&d.plugin_dir, &runner.bin)
                .context("resolve runner binary path")?;
            if !bin_path.exists() {
                println!("FAILED (runner binary)");
                println!("  Error: runner binary not found: {}", bin_path.display());
                errors += 1;
                continue;
            }
        }

        // Check processor binary exists if specified
        if let Some(proc) = &d.manifest.processors {
            let bin_path = resolve_plugin_relative_exe(&d.plugin_dir, &proc.bin)
                .context("resolve processor binary path")?;
            if !bin_path.exists() {
                println!("FAILED (processor binary)");
                println!(
                    "  Error: processor binary not found: {}",
                    bin_path.display()
                );
                errors += 1;
                continue;
            }
        }

        println!("OK");
        validated += 1;
    }

    if let Some(filter) = filter_id
        && validated == 0
        && errors == 0
    {
        println!("Plugin '{}' not found.", filter);
    }

    if errors > 0 {
        anyhow::bail!("{} validation error(s) found", errors);
    }

    println!("{} plugin(s) validated successfully.", validated);
    Ok(())
}

fn scope_root(repo_root: &Path, scope: PluginScopeArg) -> Result<PathBuf> {
    Ok(match scope {
        PluginScopeArg::Project => repo_root.join(".ralph/plugins"),
        PluginScopeArg::Global => {
            let home = std::env::var_os("HOME")
                .ok_or_else(|| anyhow::anyhow!("HOME environment variable not set"))?;
            PathBuf::from(home).join(".config/ralph/plugins")
        }
    })
}

fn cmd_install(resolved: &Resolved, source: &str, scope: PluginScopeArg) -> Result<()> {
    let source_path = Path::new(source);
    if !source_path.exists() {
        anyhow::bail!("Source path does not exist: {}", source);
    }
    if !source_path.is_dir() {
        anyhow::bail!("Source path is not a directory: {}", source);
    }

    // Validate manifest exists and is valid
    let manifest_path = source_path.join("plugin.json");
    if !manifest_path.exists() {
        anyhow::bail!("Source directory does not contain plugin.json: {}", source);
    }

    let manifest: PluginManifest = {
        let raw = fs::read_to_string(&manifest_path)
            .with_context(|| format!("read {}", manifest_path.display()))?;
        serde_json::from_str(&raw).context("parse plugin.json")?
    };
    manifest.validate().context("validate plugin manifest")?;

    let plugin_id = &manifest.id;

    // Determine target directory
    let target_root = scope_root(&resolved.repo_root, scope)?;
    let target_dir = target_root.join(plugin_id);

    // Check if already exists
    if target_dir.exists() {
        anyhow::bail!(
            "Plugin {} is already installed at {}. Use uninstall first.",
            plugin_id,
            target_dir.display()
        );
    }

    // Create target directory and copy plugin
    fs::create_dir_all(&target_root)
        .with_context(|| format!("create plugin directory {}", target_root.display()))?;

    // Copy directory recursively
    copy_dir_all(source_path, &target_dir)
        .with_context(|| format!("copy plugin to {}", target_dir.display()))?;

    println!("Installed plugin {} to {}", plugin_id, target_dir.display());
    println!();
    println!("NOTE: The plugin is NOT automatically enabled.");
    println!("To enable it, add to your config:");
    println!(
        r#"  {{ "plugins": {{ "plugins": {{ "{}": {{ "enabled": true }} }} }} }}"#,
        plugin_id
    );

    Ok(())
}

fn cmd_uninstall(resolved: &Resolved, plugin_id: &str, scope: PluginScopeArg) -> Result<()> {
    // Determine target directory
    let target_root = scope_root(&resolved.repo_root, scope)?;
    let target_dir = target_root.join(plugin_id);

    if !target_dir.exists() {
        anyhow::bail!(
            "Plugin {} is not installed at {}.",
            plugin_id,
            target_dir.display()
        );
    }

    // Verify it's actually a plugin directory
    let manifest_path = target_dir.join("plugin.json");
    if !manifest_path.exists() {
        anyhow::bail!(
            "Directory {} does not appear to be a plugin (no plugin.json).",
            target_dir.display()
        );
    }

    // Remove the directory
    fs::remove_dir_all(&target_dir)
        .with_context(|| format!("remove plugin directory {}", target_dir.display()))?;

    println!(
        "Uninstalled plugin {} from {}",
        plugin_id,
        target_dir.display()
    );

    Ok(())
}

fn default_name_from_id(id: &str) -> String {
    // "acme.super_runner" -> "acme super runner"
    id.replace(['.', '-', '_'], " ")
}

fn cmd_init(resolved: &Resolved, args: &PluginInitArgs) -> Result<()> {
    // Validate plugin ID format early
    if args.id.contains('/') || args.id.contains('\\') {
        anyhow::bail!("plugin id must not contain path separators");
    }
    if args.id.trim().is_empty() {
        anyhow::bail!("plugin id must be non-empty");
    }

    // Determine with_runner and with_processor based on flags
    let default_both = !args.with_runner && !args.with_processor;
    let with_runner = args.with_runner || default_both;
    let with_processor = args.with_processor || default_both;

    // Determine target directory
    let target_dir = if let Some(path) = &args.path {
        if path.is_absolute() {
            path.clone()
        } else {
            resolved.repo_root.join(path)
        }
    } else {
        scope_root(&resolved.repo_root, args.scope)?.join(&args.id)
    };

    // Check if target exists (unless --force)
    if target_dir.exists() && !args.force {
        anyhow::bail!(
            "Plugin directory already exists: {}. Use --force to overwrite.",
            target_dir.display()
        );
    }

    // Build manifest
    let name = args
        .name
        .clone()
        .unwrap_or_else(|| default_name_from_id(&args.id));

    let runner = if with_runner {
        Some(RunnerPlugin {
            bin: "runner.sh".to_string(),
            supports_resume: Some(false),
            default_model: None,
        })
    } else {
        None
    };

    let processors = if with_processor {
        Some(ProcessorPlugin {
            bin: "processor.sh".to_string(),
            hooks: vec![
                "validate_task".to_string(),
                "pre_prompt".to_string(),
                "post_run".to_string(),
            ],
        })
    } else {
        None
    };

    let manifest = PluginManifest {
        api_version: PLUGIN_API_VERSION,
        id: args.id.clone(),
        version: args.version.clone(),
        name,
        description: args.description.clone(),
        runner,
        processors,
    };

    // Validate the manifest before writing
    manifest.validate().context("validate generated manifest")?;

    // Prepare file contents
    let manifest_json = serde_json::to_string_pretty(&manifest)?;

    let runner_script = if with_runner {
        Some(RUNNER_SCRIPT_TEMPLATE.replace("{plugin_id}", &args.id))
    } else {
        None
    };

    let processor_script = if with_processor {
        Some(PROCESSOR_SCRIPT_TEMPLATE.replace("{plugin_id}", &args.id))
    } else {
        None
    };

    if args.dry_run {
        println!("Would create plugin directory: {}", target_dir.display());
        println!("Would write: {}", target_dir.join("plugin.json").display());
        if with_runner {
            println!("Would write: {}", target_dir.join("runner.sh").display());
        }
        if with_processor {
            println!("Would write: {}", target_dir.join("processor.sh").display());
        }
        return Ok(());
    }

    // Create directory
    fs::create_dir_all(&target_dir)
        .with_context(|| format!("create plugin directory {}", target_dir.display()))?;

    // Write files
    crate::fsutil::write_atomic(&target_dir.join("plugin.json"), manifest_json.as_bytes())
        .context("write plugin.json")?;

    if let Some(script) = runner_script {
        let runner_path = target_dir.join("runner.sh");
        crate::fsutil::write_atomic(&runner_path, script.as_bytes()).context("write runner.sh")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&runner_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&runner_path, perms)?;
        }
    }

    if let Some(script) = processor_script {
        let processor_path = target_dir.join("processor.sh");
        crate::fsutil::write_atomic(&processor_path, script.as_bytes())
            .context("write processor.sh")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&processor_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&processor_path, perms)?;
        }
    }

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
    println!();
    println!("NOTE: The plugin is NOT automatically enabled.");
    println!("To enable it, add to your config:");
    println!(
        r#"  {{ "plugins": {{ "plugins": {{ "{}": {{ "enabled": true }} }} }} }}"#,
        args.id
    );
    println!();
    println!("Validate the plugin:");
    println!("  ralph plugin validate --id {}", args.id);

    Ok(())
}

const RUNNER_SCRIPT_TEMPLATE: &str = r#"#!/bin/bash
# Runner stub for {plugin_id}
#
# Responsibilities:
# - Execute AI agent runs and resumes with prompt input from stdin.
# - Output newline-delimited JSON with text, tool_call, and finish types.
#
# Not handled here:
# - Task planning (handled by Ralph before invocation).
# - File operations outside the working directory.
#
# Assumptions:
# - stdin contains the compiled prompt.
# - Environment RALPH_PLUGIN_CONFIG_JSON contains plugin config.
# - Environment RALPH_RUNNER_CLI_JSON contains CLI options.

set -euo pipefail

PLUGIN_ID="{plugin_id}"

show_help() {
    cat << 'EOF'
Usage: runner.sh <COMMAND> [OPTIONS]

Commands:
  run       Execute a new run
  resume    Resume an existing session
  help      Show this help message

Run Options:
  --model <MODEL>             Model identifier
  --output-format <FORMAT>    Output format (must be stream-json)
  --session <ID>              Session identifier

Resume Options:
  --session <ID>              Session to resume (required)
  --model <MODEL>             Model identifier
  --output-format <FORMAT>    Output format (must be stream-json)
  <MESSAGE>                   Additional message argument

Examples:
  runner.sh run --model gpt-4 --output-format stream-json
  runner.sh resume --session abc123 --model gpt-4 --output-format stream-json "continue"
  runner.sh help

Protocol:
  Input: Prompt text via stdin
  Output: Newline-delimited JSON objects:
    {"type": "text", "content": "Hello"}
    {"type": "tool_call", "name": "write", "arguments": {"path": "file.txt"}}
    {"type": "finish", "session_id": "..."}
EOF
}

COMMAND="${1:-}"

case "$COMMAND" in
    run)
        # Stub: replace with your runner's execution logic.
        # Input prompt is provided via stdin; output must be NDJSON on stdout.
        _PROMPT=$(cat || true)
        echo "{\"type\": \"text\", \"content\": \"Stub runner: run not implemented\"}"
        echo "{\"type\": \"finish\", \"session_id\": \"stub-session\"}"
        echo "Stub runner ($PLUGIN_ID): run not implemented" >&2
        exit 1
        ;;
    resume)
        # Stub: replace with your runner's resume logic.
        echo "{\"type\": \"text\", \"content\": \"Stub runner: resume not implemented\"}"
        echo "{\"type\": \"finish\", \"session_id\": \"stub-session\"}"
        echo "Stub runner ($PLUGIN_ID): resume not implemented" >&2
        exit 1
        ;;
    help|--help|-h)
        show_help
        exit 0
        ;;
    "")
        echo "Error: No command specified" >&2
        show_help >&2
        exit 1
        ;;
    *)
        echo "Error: Unknown command: $COMMAND" >&2
        show_help >&2
        exit 1
        ;;
esac
"#;

const PROCESSOR_SCRIPT_TEMPLATE: &str = r#"#!/bin/bash
# Processor stub for {plugin_id}
#
# Responsibilities:
# - Process task lifecycle hooks: validate_task, pre_prompt, post_run.
# - Called by Ralph with hook name and task ID as arguments.
#
# Not handled here:
# - Direct task execution (handled by runners).
# - Queue modification (handled by Ralph).
#
# Assumptions:
# - First argument is the hook name.
# - Second argument is the task ID.
# - Additional arguments may follow depending on hook.

set -euo pipefail

PLUGIN_ID="{plugin_id}"

show_help() {
    cat << 'EOF'
Usage: processor.sh <HOOK> <TASK_ID> [ARGS...]

Hooks:
  validate_task    Validate task structure before execution
                   Args: <TASK_ID> <TASK_JSON_FILE>
  pre_prompt       Called before prompt is sent to runner
                   Args: <TASK_ID> <PROMPT_FILE>
  post_run         Called after runner execution completes
                   Args: <TASK_ID> <OUTPUT_FILE>

Examples:
  processor.sh validate_task RQ-0001 /tmp/task.json
  processor.sh pre_prompt RQ-0001 /tmp/prompt.txt
  processor.sh post_run RQ-0001 /tmp/output.ndjson
  processor.sh help

Exit Codes:
  0    Success
  1    Validation/processing error

Environment:
  RALPH_PLUGIN_CONFIG_JSON    Plugin configuration as JSON string
EOF
}

HOOK="${1:-}"
TASK_ID="${2:-}"

# Shift to leave remaining args for hook processing
shift 2 || true

case "$HOOK" in
    validate_task)
        # Stub: implement validate_task logic.
        # TASK_JSON_FILE="${1:-}"
        # Validate task JSON structure
        exit 0
        ;;
    pre_prompt)
        # Stub: implement pre_prompt logic.
        # PROMPT_FILE="${1:-}"
        # Can modify prompt file in place
        exit 0
        ;;
    post_run)
        # Stub: implement post_run logic.
        # OUTPUT_FILE="${1:-}"
        # Process runner output
        exit 0
        ;;
    help|--help|-h)
        show_help
        exit 0
        ;;
    "")
        echo "Error: No hook specified" >&2
        show_help >&2
        exit 1
        ;;
    *)
        echo "Error: Unknown hook: $HOOK" >&2
        show_help >&2
        exit 1
        ;;
esac
"#;

fn copy_dir_all(src: impl AsRef<Path>, dst: impl AsRef<Path>) -> Result<()> {
    fs::create_dir_all(&dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        if ty.is_dir() {
            copy_dir_all(entry.path(), dst.as_ref().join(entry.file_name()))?;
        } else {
            fs::copy(entry.path(), dst.as_ref().join(entry.file_name()))?;
        }
    }
    Ok(())
}
