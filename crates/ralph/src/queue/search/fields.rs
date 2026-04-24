//! Field iteration helpers for task searching.
//!
//! Purpose:
//! - Field iteration helpers for task searching.
//!
//! Responsibilities:
//! - Provide a callback-based helper to iterate over all searchable text fields
//!
//! Not handled here:
//! - Actual matching logic (handled by substring and fuzzy modules)
//! - Field filtering or selection
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - All text fields are visited exactly once per task
//! - Empty fields are still passed to the callback (caller decides handling)
//! - Field order is: title, evidence[], plan[], notes[], request?, tags[], scope[], custom_fields (k,v)

use crate::contracts::Task;

/// Iterate over all searchable text fields in a task, calling `f` for each.
///
/// This avoids iterator lifetime complexity while ensuring consistent
/// field coverage across substring and fuzzy search implementations.
///
/// Fields visited in order:
/// 1. title
/// 2. evidence[] (each element)
/// 3. plan[] (each element)
/// 4. notes[] (each element)
/// 5. request? (if Some)
/// 6. tags[] (each element)
/// 7. scope[] (each element)
/// 8. custom_fields (each key and value)
pub fn for_each_searchable_text<F>(task: &Task, mut f: F)
where
    F: FnMut(&str),
{
    f(&task.title);
    task.evidence.iter().for_each(|e| f(e));
    task.plan.iter().for_each(|p| f(p));
    task.notes.iter().for_each(|n| f(n));
    if let Some(req) = &task.request {
        f(req);
    }
    task.tags.iter().for_each(|t| f(t));
    task.scope.iter().for_each(|s| f(s));
    task.custom_fields.iter().for_each(|(k, v)| {
        f(k);
        f(v);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Task, TaskStatus};
    use std::collections::HashMap;

    fn test_task() -> Task {
        Task {
            id: "RQ-0001".to_string(),
            status: TaskStatus::Todo,
            title: "Test title".to_string(),
            description: None,
            priority: Default::default(),
            tags: vec!["tag1".to_string(), "tag2".to_string()],
            scope: vec!["scope1".to_string()],
            evidence: vec!["evidence1".to_string()],
            plan: vec!["plan1".to_string(), "plan2".to_string()],
            notes: vec!["note1".to_string()],
            request: Some("request text".to_string()),
            agent: None,
            created_at: Some("2026-01-18T00:00:00Z".to_string()),
            updated_at: Some("2026-01-18T00:00:00Z".to_string()),
            completed_at: None,
            started_at: None,
            scheduled_start: None,
            estimated_minutes: None,
            actual_minutes: None,
            depends_on: vec![],
            blocks: vec![],
            relates_to: vec![],
            duplicates: None,
            custom_fields: {
                let mut m = HashMap::new();
                m.insert("key1".to_string(), "value1".to_string());
                m
            },
            parent_id: None,
        }
    }

    #[test]
    fn for_each_collects_all_fields() {
        let task = test_task();
        let mut collected = Vec::new();

        for_each_searchable_text(&task, |text| collected.push(text.to_string()));

        // Should have: title + 1 evidence + 2 plan + 1 notes + 1 request + 2 tags + 1 scope + 2 custom_fields
        assert_eq!(collected.len(), 11);
        assert!(collected.contains(&"Test title".to_string()));
        assert!(collected.contains(&"evidence1".to_string()));
        assert!(collected.contains(&"plan1".to_string()));
        assert!(collected.contains(&"plan2".to_string()));
        assert!(collected.contains(&"note1".to_string()));
        assert!(collected.contains(&"request text".to_string()));
        assert!(collected.contains(&"tag1".to_string()));
        assert!(collected.contains(&"tag2".to_string()));
        assert!(collected.contains(&"scope1".to_string()));
        assert!(collected.contains(&"key1".to_string()));
        assert!(collected.contains(&"value1".to_string()));
    }

    #[test]
    fn for_each_handles_empty_optional_fields() {
        let mut task = test_task();
        task.request = None;
        task.evidence.clear();
        task.custom_fields.clear();

        let mut count = 0;
        for_each_searchable_text(&task, |_| count += 1);

        // Should have: title + 0 evidence + 2 plan + 1 notes + 0 request + 2 tags + 1 scope + 0 custom
        assert_eq!(count, 7);
    }
}
