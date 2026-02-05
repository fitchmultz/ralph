//! Shared helpers for interactive runner/scan factory setup.
//!
//! Responsibilities:
//! - Build runner/scan factory closures for interactive execution paths.
//! - Resolve RepoPrompt tooling injection for scans in interactive flows.
//!
//! Not handled here:
//! - CLI argument parsing or command dispatch.
//! - Queue persistence or task status updates.
//! - Runner process execution details.
//!
//! Invariants/assumptions:
//! - Callers provide resolved config and validated agent overrides.
//! - RepoPrompt mode selections already passed through CLI normalization.

use anyhow::Result;
use std::sync::Arc;

use crate::{
    agent,
    cli::scan::ScanMode,
    commands::{run as run_cmd, scan as scan_cmd},
    config, runner, runutil,
};

/// Factory that prepares a task runner closure for interactive execution.
pub type RunnerFactory = Box<
    dyn Fn(
            String,
            runner::OutputHandler,
            runutil::RevertPromptHandler,
        ) -> Box<dyn FnOnce() -> Result<()> + Send>
        + Send
        + Sync,
>;

/// Factory that prepares a scan runner closure for interactive execution.
pub type ScanFactory = Box<
    dyn Fn(
            String,
            runner::OutputHandler,
            runutil::RevertPromptHandler,
        ) -> Box<dyn FnOnce() -> Result<()> + Send>
        + Send
        + Sync,
>;

/// Callable used by the runner factory to execute a task with a locked queue.
pub type RunInvoker = dyn Fn(
        &config::Resolved,
        &agent::AgentOverrides,
        bool,
        &str,
        Option<runner::OutputHandler>,
        Option<runutil::RevertPromptHandler>,
    ) -> Result<()>
    + Send
    + Sync;

/// Callable used by the scan factory to execute a scan with a locked queue.
pub type ScanInvoker = dyn Fn(&config::Resolved, scan_cmd::ScanOptions) -> Result<()> + Send + Sync;

/// Bundled runner + scan factories for interactive flows.
pub struct InteractiveFactories {
    pub runner_factory: RunnerFactory,
    pub scan_factory: ScanFactory,
}

/// Build interactive factories with production run/scan invokers.
pub fn build_interactive_factories(
    resolved: &config::Resolved,
    overrides: &agent::AgentOverrides,
    repo_prompt: Option<agent::RepoPromptMode>,
    force: bool,
) -> Result<InteractiveFactories> {
    build_interactive_factories_with_invokers(
        resolved,
        overrides,
        repo_prompt,
        force,
        Arc::new(run_cmd::run_one_with_id_locked),
        Arc::new(scan_cmd::run_scan),
    )
}

/// Build interactive factories with injected invokers (useful for tests).
pub fn build_interactive_factories_with_invokers(
    resolved: &config::Resolved,
    overrides: &agent::AgentOverrides,
    repo_prompt: Option<agent::RepoPromptMode>,
    force: bool,
    run_invoker: Arc<RunInvoker>,
    scan_invoker: Arc<ScanInvoker>,
) -> Result<InteractiveFactories> {
    let scan_repoprompt_tool_injection = agent::resolve_rp_required(repo_prompt, resolved);
    let scan_git_revert_mode = overrides
        .git_revert_mode
        .or(resolved.config.agent.git_revert_mode)
        .unwrap_or(crate::contracts::GitRevertMode::Ask);

    let resolved_for_run = resolved.clone();
    let overrides_for_run = overrides.clone();
    let run_invoker_for_factory = Arc::clone(&run_invoker);
    let runner_factory: RunnerFactory = Box::new(move |task_id, handler, revert_prompt| {
        let resolved = resolved_for_run.clone();
        let overrides = overrides_for_run.clone();
        let run_invoker = Arc::clone(&run_invoker_for_factory);
        let force = force;
        Box::new(move || {
            (run_invoker)(
                &resolved,
                &overrides,
                force,
                &task_id,
                Some(handler),
                Some(revert_prompt),
            )
        })
    });

    let resolved_for_scan = resolved.clone();
    let scan_overrides = overrides.clone();
    let scan_invoker_for_factory = Arc::clone(&scan_invoker);
    let repoprompt_tool_injection = scan_repoprompt_tool_injection;
    let git_revert_mode = scan_git_revert_mode;
    let scan_factory: ScanFactory = Box::new(move |focus, handler, revert_prompt| {
        let resolved = resolved_for_scan.clone();
        let overrides = scan_overrides.clone();
        let scan_invoker = Arc::clone(&scan_invoker_for_factory);
        let force = force;
        let repoprompt_tool_injection = repoprompt_tool_injection;
        let git_revert_mode = git_revert_mode;
        Box::new(move || {
            (scan_invoker)(
                &resolved,
                scan_cmd::ScanOptions {
                    focus,
                    mode: ScanMode::Maintenance,
                    runner_override: overrides.runner,
                    model_override: overrides.model,
                    reasoning_effort_override: overrides.reasoning_effort,
                    runner_cli_overrides: overrides.runner_cli,
                    force,
                    repoprompt_tool_injection,
                    git_revert_mode,
                    lock_mode: scan_cmd::ScanLockMode::Held,
                    output_handler: Some(handler),
                    revert_prompt: Some(revert_prompt),
                },
            )
        })
    });

    Ok(InteractiveFactories {
        runner_factory,
        scan_factory,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Config, GitRevertMode, Model, ReasoningEffort, Runner};
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    struct RunCall {
        repo_root: PathBuf,
        runner: Option<Runner>,
        model: Option<Model>,
        reasoning_effort: Option<ReasoningEffort>,
        force: bool,
        task_id: String,
        output_handler: bool,
        revert_prompt: bool,
    }

    struct ScanCall {
        repo_root: PathBuf,
        options: scan_cmd::ScanOptions,
    }

    fn resolved_with_config(config: Config) -> (config::Resolved, TempDir) {
        let dir = TempDir::new().expect("temp dir");
        let repo_root = dir.path().to_path_buf();
        let queue_rel = config
            .queue
            .file
            .clone()
            .unwrap_or_else(|| PathBuf::from(".ralph/queue.json"));
        let done_rel = config
            .queue
            .done_file
            .clone()
            .unwrap_or_else(|| PathBuf::from(".ralph/done.json"));
        let id_prefix = config
            .queue
            .id_prefix
            .clone()
            .unwrap_or_else(|| "RQ".to_string());
        let id_width = config.queue.id_width.unwrap_or(4) as usize;

        (
            config::Resolved {
                config,
                repo_root: repo_root.clone(),
                queue_path: repo_root.join(queue_rel),
                done_path: repo_root.join(done_rel),
                id_prefix,
                id_width,
                global_config_path: None,
                project_config_path: Some(repo_root.join(".ralph/config.json")),
            },
            dir,
        )
    }

    fn handler() -> runner::OutputHandler {
        Arc::new(Box::new(|_text: &str| {}))
    }

    fn revert_prompt() -> runutil::RevertPromptHandler {
        Arc::new(|_context: &runutil::RevertPromptContext| runutil::RevertDecision::Keep)
    }

    #[test]
    fn interactive_factories_pass_expected_arguments() {
        let mut config = Config::default();
        config.agent.runner = Some(Runner::Codex);
        config.agent.model = Some(Model::Gpt52Codex);
        config.agent.reasoning_effort = Some(ReasoningEffort::High);
        config.agent.repoprompt_tool_injection = Some(true);
        config.agent.git_revert_mode = Some(GitRevertMode::Enabled);

        let (resolved, _dir) = resolved_with_config(config);
        let overrides = agent::AgentOverrides {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt52Codex),
            reasoning_effort: Some(ReasoningEffort::High),
            git_revert_mode: None,
            ..Default::default()
        };

        let run_capture: Arc<Mutex<Option<RunCall>>> = Arc::new(Mutex::new(None));
        let scan_capture: Arc<Mutex<Option<ScanCall>>> = Arc::new(Mutex::new(None));

        let run_invoker: Arc<RunInvoker> = {
            let run_capture = Arc::clone(&run_capture);
            Arc::new(
                move |resolved, overrides, force, task_id, output_handler, revert_prompt| {
                    let call = RunCall {
                        repo_root: resolved.repo_root.clone(),
                        runner: overrides.runner.clone(),
                        model: overrides.model.clone(),
                        reasoning_effort: overrides.reasoning_effort,
                        force,
                        task_id: task_id.to_string(),
                        output_handler: output_handler.is_some(),
                        revert_prompt: revert_prompt.is_some(),
                    };
                    *run_capture.lock().expect("run capture") = Some(call);
                    Ok(())
                },
            )
        };

        let scan_invoker: Arc<ScanInvoker> = {
            let scan_capture = Arc::clone(&scan_capture);
            Arc::new(move |resolved, options| {
                let call = ScanCall {
                    repo_root: resolved.repo_root.clone(),
                    options,
                };
                *scan_capture.lock().expect("scan capture") = Some(call);
                Ok(())
            })
        };

        let factories = build_interactive_factories_with_invokers(
            &resolved,
            &overrides,
            Some(agent::RepoPromptMode::Off),
            true,
            run_invoker,
            scan_invoker,
        )
        .expect("factories");

        let runner = (factories.runner_factory)("RQ-0007".to_string(), handler(), revert_prompt());
        runner().expect("runner invoked");

        let scan =
            (factories.scan_factory)("duplication audit".to_string(), handler(), revert_prompt());
        scan().expect("scan invoked");

        let run_call = run_capture
            .lock()
            .expect("run capture lock")
            .take()
            .expect("run call");
        assert_eq!(run_call.repo_root, resolved.repo_root);
        assert_eq!(run_call.runner, Some(Runner::Codex));
        assert_eq!(run_call.model, Some(Model::Gpt52Codex));
        assert_eq!(run_call.reasoning_effort, Some(ReasoningEffort::High));
        assert!(run_call.force);
        assert_eq!(run_call.task_id, "RQ-0007");
        assert!(run_call.output_handler);
        assert!(run_call.revert_prompt);

        let scan_call = scan_capture
            .lock()
            .expect("scan capture lock")
            .take()
            .expect("scan call");
        assert_eq!(scan_call.repo_root, resolved.repo_root);
        assert_eq!(scan_call.options.focus, "duplication audit");
        assert_eq!(scan_call.options.runner_override, Some(Runner::Codex));
        assert_eq!(scan_call.options.model_override, Some(Model::Gpt52Codex));
        assert_eq!(
            scan_call.options.reasoning_effort_override,
            Some(ReasoningEffort::High)
        );
        assert!(scan_call.options.force);
        assert!(!scan_call.options.repoprompt_tool_injection);
        assert_eq!(scan_call.options.git_revert_mode, GitRevertMode::Enabled);
        assert_eq!(scan_call.options.lock_mode, scan_cmd::ScanLockMode::Held);
        assert!(scan_call.options.output_handler.is_some());
        assert!(scan_call.options.revert_prompt.is_some());
    }

    #[test]
    fn interactive_factories_pass_empty_scan_overrides_when_missing() {
        let mut config = Config::default();
        config.agent.runner = Some(Runner::Gemini);
        config.agent.model = Some(Model::Custom("gemini-3-flash-preview".to_string()));
        config.agent.reasoning_effort = Some(ReasoningEffort::High);
        config.agent.repoprompt_tool_injection = Some(true);
        config.agent.git_revert_mode = Some(GitRevertMode::Disabled);

        let (resolved, _dir) = resolved_with_config(config);
        let overrides = agent::AgentOverrides::default();

        let scan_capture: Arc<Mutex<Option<ScanCall>>> = Arc::new(Mutex::new(None));
        let scan_invoker: Arc<ScanInvoker> = {
            let scan_capture = Arc::clone(&scan_capture);
            Arc::new(move |resolved, options| {
                let call = ScanCall {
                    repo_root: resolved.repo_root.clone(),
                    options,
                };
                *scan_capture.lock().expect("scan capture") = Some(call);
                Ok(())
            })
        };

        let run_invoker: Arc<RunInvoker> = Arc::new(|_, _, _, _, _, _| Ok(()));

        let factories = build_interactive_factories_with_invokers(
            &resolved,
            &overrides,
            None,
            false,
            run_invoker,
            scan_invoker,
        )
        .expect("factories");

        let scan = (factories.scan_factory)("queue audit".to_string(), handler(), revert_prompt());
        scan().expect("scan invoked");

        let scan_call = scan_capture
            .lock()
            .expect("scan capture lock")
            .take()
            .expect("scan call");
        assert_eq!(scan_call.repo_root, resolved.repo_root);
        assert!(scan_call.options.runner_override.is_none());
        assert!(scan_call.options.model_override.is_none());
        assert!(scan_call.options.reasoning_effort_override.is_none());
        assert!(!scan_call.options.force);
        assert!(scan_call.options.repoprompt_tool_injection);
        assert_eq!(scan_call.options.git_revert_mode, GitRevertMode::Disabled);
    }
}
