//! Tree normalization helpers for task decomposition.
//!
//! Purpose:
//! - Tree normalization helpers for task decomposition.
//!
//! Responsibilities:
//! - Normalize planner JSON into bounded Ralph decomposition trees.
//! - Resolve sibling dependency references within normalized child groups.
//! - Produce preview metadata like node counts and dependency edges.
//!
//! Not handled here:
//! - Runner invocation or prompt rendering.
//! - Queue writes or task ID materialization.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Dependency inference remains limited to sibling tasks within the same parent group.
//! - Warning emission is deduplicated for stable preview output.

use super::support::{
    collect_dependency_edges, count_nodes, normalize_key, normalize_optional_string,
    normalize_strings, normalize_title, push_warning,
};
use super::types::{
    DecompositionPlan, PlannedNode, PlannerState, RawDecompositionResponse, RawPlannedNode,
    SourceKind, TaskDecomposeOptions,
};
use anyhow::{Context, Result};
use std::collections::HashMap;

pub(super) fn normalize_response(
    raw: RawDecompositionResponse,
    source_kind: SourceKind,
    opts: &TaskDecomposeOptions,
    default_root_title: &str,
) -> Result<DecompositionPlan> {
    let mut state = PlannerState {
        remaining_nodes: opts.max_nodes,
        warnings: normalize_strings(raw.warnings),
        with_dependencies: opts.with_dependencies,
    };
    let root = normalize_node(
        raw.tree,
        1,
        opts.max_depth,
        opts.max_children,
        &mut state,
        false,
        default_root_title,
    )
    .context("Task decomposition planner returned an empty tree.")?;
    let (total_nodes, leaf_nodes) = count_nodes(&root);
    if source_kind == SourceKind::ExistingTask && root.children.is_empty() {
        push_warning(
            &mut state.warnings,
            "Planner produced no child tasks for the existing parent.",
        );
    }
    let dependency_edges = collect_dependency_edges(&root);
    Ok(DecompositionPlan {
        root,
        warnings: state.warnings,
        total_nodes,
        leaf_nodes,
        dependency_edges,
    })
}

fn normalize_node(
    raw: RawPlannedNode,
    depth: u8,
    max_depth: u8,
    max_children: usize,
    state: &mut PlannerState,
    allow_collapse: bool,
    default_title: &str,
) -> Option<PlannedNode> {
    if state.remaining_nodes == 0 {
        push_warning(
            &mut state.warnings,
            "Planner output exceeded the max-nodes cap; remaining descendants were dropped.",
        );
        return None;
    }
    state.remaining_nodes -= 1;

    let title = normalize_title(&raw.title, default_title);
    let mut node = PlannedNode {
        planner_key: normalize_key(raw.key.as_deref(), &title),
        title,
        description: normalize_optional_string(raw.description),
        plan: normalize_strings(raw.plan),
        tags: normalize_strings(raw.tags),
        scope: normalize_strings(raw.scope),
        depends_on_keys: Vec::new(),
        children: Vec::new(),
        dependency_refs: normalize_strings(raw.depends_on),
    };

    if depth >= max_depth {
        if !raw.children.is_empty() {
            push_warning(
                &mut state.warnings,
                &format!(
                    "Depth cap {} reached at '{}'; deeper descendants were dropped.",
                    max_depth, node.title
                ),
            );
        }
    } else {
        node.children = normalize_children(raw.children, depth + 1, max_depth, max_children, state);
    }

    if allow_collapse
        && node.children.len() == 1
        && node.description.is_none()
        && node.plan.is_empty()
        && node.tags.is_empty()
        && node.scope.is_empty()
        && node.depends_on_keys.is_empty()
        && node.dependency_refs.is_empty()
    {
        push_warning(
            &mut state.warnings,
            &format!(
                "Collapsed a degenerate single-child chain under '{}'.",
                node.title
            ),
        );
        return node.children.into_iter().next();
    }

    Some(node)
}

fn normalize_children(
    raw_children: Vec<RawPlannedNode>,
    depth: u8,
    max_depth: u8,
    max_children: usize,
    state: &mut PlannerState,
) -> Vec<PlannedNode> {
    let total_children = raw_children.len();
    let mut children = Vec::new();
    for child in raw_children.into_iter().take(max_children) {
        if let Some(normalized_child) = normalize_node(
            child,
            depth,
            max_depth,
            max_children,
            state,
            true,
            "Untitled task",
        ) {
            children.push(normalized_child);
        }
    }
    if total_children > max_children {
        push_warning(
            &mut state.warnings,
            &format!(
                "Child cap {} reached; extra siblings were dropped.",
                max_children
            ),
        );
    }
    uniquify_planner_keys(&mut children);
    if state.with_dependencies {
        resolve_sibling_dependencies(&mut children, &mut state.warnings);
    } else {
        clear_dependency_refs(&mut children);
    }
    children
}

fn clear_dependency_refs(children: &mut [PlannedNode]) {
    for child in children {
        child.dependency_refs.clear();
    }
}

fn uniquify_planner_keys(children: &mut [PlannedNode]) {
    let mut seen = HashMap::<String, usize>::new();
    for child in children {
        let base = child.planner_key.clone();
        let count = seen.entry(base.clone()).or_insert(0);
        if *count > 0 {
            child.planner_key = format!("{base}-{}", *count + 1);
        }
        *count += 1;
    }
}

fn resolve_sibling_dependencies(children: &mut [PlannedNode], warnings: &mut Vec<String>) {
    let key_map = children
        .iter()
        .map(|child| (child.planner_key.clone(), child.title.clone()))
        .collect::<HashMap<_, _>>();
    let title_map = children
        .iter()
        .map(|child| (normalize_key(None, &child.title), child.planner_key.clone()))
        .collect::<HashMap<_, _>>();

    for child in children.iter_mut() {
        let mut resolved = Vec::new();
        for dep in child.dependency_refs.drain(..) {
            let normalized_dep = normalize_key(Some(dep.as_str()), dep.as_str());
            let candidate = if key_map.contains_key(&normalized_dep) {
                Some(normalized_dep.clone())
            } else {
                title_map.get(&normalized_dep).cloned()
            };
            match candidate {
                Some(dep_key) if dep_key == child.planner_key => push_warning(
                    warnings,
                    &format!(
                        "Dropped self-dependency on '{}' within sibling dependency inference.",
                        child.title
                    ),
                ),
                Some(dep_key) => {
                    if !resolved.iter().any(|existing| existing == &dep_key) {
                        resolved.push(dep_key);
                    }
                }
                None => push_warning(
                    warnings,
                    &format!(
                        "Dropped unknown sibling dependency '{}' from '{}'.",
                        dep, child.title
                    ),
                ),
            }
        }
        child.depends_on_keys = resolved;
    }
}
