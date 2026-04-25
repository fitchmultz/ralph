//! Purpose: Apply discovery follow-up proposals into the active task queue.
//!
//! Responsibilities:
//! - Parse `followups@v1` proposal documents from `.ralph/cache/followups`.
//! - Validate proposal-local keys, dependency references, and source-task binding.
//! - Materialize proposal entries as normal queue tasks with allocated IDs.
//! - Persist validated queue updates and remove applied proposal artifacts.
//!
//! Scope:
//! - Queue-growth handoff only; task building, runner prompting, and task completion live elsewhere.
//! - Follow-up proposals never edit existing tasks or the done archive.
//!
//! Usage:
//! - CLI: `ralph task followups apply --task <TASK_ID>`.
//! - Parallel integration: apply a worker-local proposal after archiving the completed task.
//!
//! Invariants/Assumptions:
//! - Proposal keys are local to one proposal document and must be unique after trimming.
//! - All `depends_on_keys` references must point at proposal-local keys.
//! - Source-task provenance uses the existing `request` and `relates_to` task fields.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use crate::config::Resolved;
use crate::contracts::{QueueFile, Task, TaskPriority, TaskStatus};
use crate::queue::operations::{
    MaterializeInsertion, MaterializeTaskGraphOptions, MaterializedTaskSpec,
    apply_materialized_task_graph,
};
use crate::{jsonc, queue};

const FOLLOWUPS_VERSION: u8 = 1;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FollowupProposalDocument {
    pub version: u8,
    pub source_task_id: String,
    pub tasks: Vec<FollowupTaskProposal>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FollowupTaskProposal {
    pub key: String,
    pub title: String,
    pub description: String,
    pub priority: TaskPriority,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub scope: Vec<String>,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub plan: Vec<String>,
    #[serde(default)]
    pub depends_on_keys: Vec<String>,
    pub independence_rationale: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FollowupApplyReport {
    pub version: u8,
    pub dry_run: bool,
    pub source_task_id: String,
    pub proposal_path: String,
    pub created_tasks: Vec<FollowupCreatedTask>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FollowupCreatedTask {
    pub key: String,
    pub task_id: String,
    pub title: String,
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct FollowupApplyOptions<'a> {
    pub task_id: &'a str,
    pub input_path: Option<&'a Path>,
    pub dry_run: bool,
    pub create_undo: bool,
    pub remove_proposal: bool,
}

pub fn default_followups_path(repo_root: &Path, task_id: &str) -> PathBuf {
    repo_root
        .join(".ralph")
        .join("cache")
        .join("followups")
        .join(format!("{}.json", task_id.trim()))
}

pub fn apply_default_followups_if_present(
    resolved: &Resolved,
    task_id: &str,
) -> Result<Option<FollowupApplyReport>> {
    apply_default_followups_if_present_with_removal(resolved, task_id, true)
}

pub fn apply_default_followups_if_present_with_removal(
    resolved: &Resolved,
    task_id: &str,
    remove_proposal: bool,
) -> Result<Option<FollowupApplyReport>> {
    let path = default_followups_path(&resolved.repo_root, task_id);
    if !path.exists() {
        return Ok(None);
    }

    apply_followups_file(
        resolved,
        &FollowupApplyOptions {
            task_id,
            input_path: Some(path.as_path()),
            dry_run: false,
            create_undo: false,
            remove_proposal,
        },
    )
    .map(Some)
}

pub fn remove_default_followups_proposal_if_present(repo_root: &Path, task_id: &str) -> Result<()> {
    remove_applied_proposal(&default_followups_path(repo_root, task_id))
}

pub fn apply_followups_file(
    resolved: &Resolved,
    opts: &FollowupApplyOptions<'_>,
) -> Result<FollowupApplyReport> {
    let source_task_id = normalize_required(opts.task_id, "task id")?;
    let path = opts
        .input_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| default_followups_path(&resolved.repo_root, source_task_id));
    let document = read_followups_document(&path)?;

    let mut active = queue::load_queue(&resolved.queue_path)
        .with_context(|| format!("load queue {}", resolved.queue_path.display()))?;
    let done = queue::load_queue_or_default(&resolved.done_path)
        .with_context(|| format!("load done {}", resolved.done_path.display()))?;
    let done_ref = queue::optional_done_queue(&done, &resolved.done_path);
    let now = crate::timeutil::now_utc_rfc3339()?;

    let report = apply_followups_in_memory(
        &mut active,
        done_ref,
        &document,
        source_task_id,
        &path,
        &now,
        &resolved.id_prefix,
        resolved.id_width,
        resolved.config.queue.max_dependency_depth.unwrap_or(10),
        opts.dry_run,
    )?;

    if opts.dry_run {
        return Ok(report);
    }

    if opts.create_undo {
        crate::undo::create_undo_snapshot(
            resolved,
            &format!(
                "task followups apply [{} task(s)]",
                report.created_tasks.len()
            ),
        )?;
    }
    queue::save_queue(&resolved.queue_path, &active)
        .with_context(|| format!("save queue {}", resolved.queue_path.display()))?;

    if opts.remove_proposal {
        remove_applied_proposal(&path)?;
    }

    Ok(report)
}

#[allow(clippy::too_many_arguments)]
pub fn apply_followups_in_memory(
    active: &mut QueueFile,
    done: Option<&QueueFile>,
    document: &FollowupProposalDocument,
    expected_source_task_id: &str,
    proposal_path: &Path,
    now_rfc3339: &str,
    id_prefix: &str,
    id_width: usize,
    max_dependency_depth: u8,
    dry_run: bool,
) -> Result<FollowupApplyReport> {
    let source_task_id = validate_document_header(document, expected_source_task_id)?;
    let source_task = find_source_task(active, done, source_task_id)?;
    let source_request = source_task.request.clone();

    validate_proposal_tasks(document)?;
    let specs = materialized_followup_specs(document, source_task_id, source_request)?;
    let report = apply_materialized_task_graph(
        active,
        done,
        &specs,
        &MaterializeTaskGraphOptions {
            now_rfc3339,
            id_prefix,
            id_width,
            max_dependency_depth,
            insertion: MaterializeInsertion::QueueDefaultTop,
            dry_run,
        },
    )?;
    let mut created = Vec::with_capacity(report.created_tasks.len());
    for spec in &specs {
        let key = normalize_required(&spec.local_key, "follow-up key")?.to_string();
        let task = report
            .created_tasks
            .iter()
            .find(|task| task.id == report.local_key_to_id[&key])
            .ok_or_else(|| anyhow!("missing materialized follow-up task for key {key}"))?;
        created.push(FollowupCreatedTask {
            key: key.clone(),
            task_id: task.id.clone(),
            title: task.title.clone(),
            depends_on: task.depends_on.clone(),
        });
    }

    Ok(FollowupApplyReport {
        version: FOLLOWUPS_VERSION,
        dry_run,
        source_task_id: source_task_id.to_string(),
        proposal_path: proposal_path.display().to_string(),
        created_tasks: created,
    })
}

fn read_followups_document(path: &Path) -> Result<FollowupProposalDocument> {
    let raw = fs::read_to_string(path)
        .with_context(|| format!("read follow-up proposal {}", path.display()))?;
    jsonc::parse_jsonc::<FollowupProposalDocument>(
        &raw,
        &format!("follow-up proposal {}", path.display()),
    )
}

fn validate_document_header<'a>(
    document: &'a FollowupProposalDocument,
    expected_source_task_id: &str,
) -> Result<&'a str> {
    if document.version != FOLLOWUPS_VERSION {
        bail!(
            "Unsupported followups proposal version: {}. Ralph requires version {}.",
            document.version,
            FOLLOWUPS_VERSION
        );
    }

    let source_task_id = normalize_required(&document.source_task_id, "source_task_id")?;
    let expected = normalize_required(expected_source_task_id, "task id")?;
    if source_task_id != expected {
        bail!(
            "follow-up proposal source_task_id {} does not match --task {}",
            source_task_id,
            expected
        );
    }

    Ok(source_task_id)
}

fn materialized_followup_specs(
    document: &FollowupProposalDocument,
    source_task_id: &str,
    source_request: Option<String>,
) -> Result<Vec<MaterializedTaskSpec>> {
    document
        .tasks
        .iter()
        .map(|proposal| {
            let key = normalize_required(&proposal.key, "follow-up key")?.to_string();
            Ok(MaterializedTaskSpec {
                local_key: key.clone(),
                title: normalize_required(&proposal.title, "follow-up title")?.to_string(),
                description: Some(
                    normalize_required(&proposal.description, "follow-up description")?.to_string(),
                ),
                priority: proposal.priority,
                status: TaskStatus::Todo,
                tags: proposal.tags.clone(),
                scope: proposal.scope.clone(),
                evidence: proposal.evidence.clone(),
                plan: proposal.plan.clone(),
                notes: vec![format!("Generated from follow-up proposal key {key}")],
                request: source_request.clone(),
                relates_to: vec![source_task_id.to_string()],
                parent_local_key: None,
                parent_task_id: None,
                depends_on_local_keys: proposal.depends_on_keys.clone(),
                estimated_minutes: None,
            })
        })
        .collect()
}

fn validate_proposal_tasks(document: &FollowupProposalDocument) -> Result<()> {
    let mut keys = std::collections::HashSet::with_capacity(document.tasks.len());
    for proposal in &document.tasks {
        let key = normalize_required(&proposal.key, "follow-up key")?;
        if !keys.insert(key.to_string()) {
            bail!("duplicate follow-up proposal key: {key}");
        }
        normalize_required(&proposal.title, "follow-up title")?;
        normalize_required(&proposal.description, "follow-up description")?;
        normalize_required(
            &proposal.independence_rationale,
            "follow-up independence_rationale",
        )?;
    }
    Ok(())
}

fn find_source_task<'a>(
    active: &'a QueueFile,
    done: Option<&'a QueueFile>,
    source_task_id: &str,
) -> Result<&'a Task> {
    active
        .tasks
        .iter()
        .find(|task| task.id.trim() == source_task_id)
        .or_else(|| {
            done.and_then(|done| {
                done.tasks
                    .iter()
                    .find(|task| task.id.trim() == source_task_id)
            })
        })
        .ok_or_else(|| {
            anyhow!(
                "{}",
                crate::error_messages::task_not_found_in_queue_or_done(source_task_id)
            )
        })
}

fn normalize_required<'a>(value: &'a str, label: &str) -> Result<&'a str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("{label} must be non-empty");
    }
    Ok(trimmed)
}

fn remove_applied_proposal(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err).with_context(|| format!("remove applied proposal {}", path.display())),
    }
}
