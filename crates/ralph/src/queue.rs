use crate::contracts::{QueueFile, Task, TaskStatus};
use crate::fsutil;
use crate::redaction;
use crate::timeutil;
use anyhow::{anyhow, bail, Context, Result};
use std::collections::HashSet;
use std::path::Path;
use time::format_description::well_known::Rfc3339;
use time::OffsetDateTime;

#[derive(Debug, Clone)]
pub struct ArchiveReport {
    pub moved_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct RepairReport {
    pub repaired: bool,
}

pub fn acquire_queue_lock(repo_root: &Path, label: &str, force: bool) -> Result<fsutil::DirLock> {
    let lock_dir = fsutil::queue_lock_dir(repo_root);
    fsutil::acquire_dir_lock(&lock_dir, label, force)
}

pub fn load_queue_or_default_with_repair(
    path: &Path,
    id_prefix: &str,
    id_width: usize,
) -> Result<(QueueFile, bool)> {
    if !path.exists() {
        return Ok((QueueFile::default(), false));
    }
    load_queue_with_repair(path, id_prefix, id_width)
}

pub fn warn_if_repaired(path: &Path, repaired: bool) {
    if repaired {
        log::warn!("Repaired queue YAML format issues in {}", path.display());
    }
}

pub fn load_queue_with_repair(
    path: &Path,
    id_prefix: &str,
    id_width: usize,
) -> Result<(QueueFile, bool)> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read queue file {}", path.display()))?;
    let parsed = serde_yaml::from_str::<QueueFile>(&raw);
    match parsed {
        Ok(queue) => {
            if let Some(repaired) = repair_yaml(&raw, id_prefix, id_width) {
                let queue: QueueFile = serde_yaml::from_str(&repaired)
                    .with_context(|| format!("parse repaired queue YAML {}", path.display()))?;
                fsutil::write_atomic(path, repaired.as_bytes())
                    .with_context(|| format!("write repaired queue YAML {}", path.display()))?;
                Ok((queue, true))
            } else {
                Ok((queue, false))
            }
        }
        Err(err) => {
            let repaired = repair_yaml(&raw, id_prefix, id_width)
                .ok_or_else(|| anyhow!("parse queue YAML {}: {err}", path.display()))?;
            let queue: QueueFile = serde_yaml::from_str(&repaired)
                .with_context(|| format!("parse repaired queue YAML {}", path.display()))?;
            fsutil::write_atomic(path, repaired.as_bytes())
                .with_context(|| format!("write repaired queue YAML {}", path.display()))?;
            Ok((queue, true))
        }
    }
}

pub fn repair_queue(path: &Path, id_prefix: &str, id_width: usize) -> Result<RepairReport> {
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("read queue file {}", path.display()))?;
    let parsed = serde_yaml::from_str::<QueueFile>(&raw);
    match parsed {
        Ok(_) => {
            if let Some(repaired) = repair_yaml(&raw, id_prefix, id_width) {
                let _queue: QueueFile = serde_yaml::from_str(&repaired)
                    .with_context(|| format!("parse repaired queue YAML {}", path.display()))?;
                fsutil::write_atomic(path, repaired.as_bytes())
                    .with_context(|| format!("write repaired queue YAML {}", path.display()))?;
                Ok(RepairReport { repaired: true })
            } else {
                Ok(RepairReport { repaired: false })
            }
        }
        Err(_) => {
            let repaired = repair_yaml(&raw, id_prefix, id_width)
                .ok_or_else(|| anyhow!("unable to repair queue YAML {}", path.display()))?;
            let _queue: QueueFile = serde_yaml::from_str(&repaired)
                .with_context(|| format!("parse repaired queue YAML {}", path.display()))?;
            fsutil::write_atomic(path, repaired.as_bytes())
                .with_context(|| format!("write repaired queue YAML {}", path.display()))?;
            Ok(RepairReport { repaired: true })
        }
    }
}

pub fn save_queue(path: &Path, queue: &QueueFile) -> Result<()> {
    let rendered = serde_yaml::to_string(queue).context("serialize queue YAML")?;
    let rendered = repair_yaml_scalars(&rendered).unwrap_or(rendered);
    fsutil::write_atomic(path, rendered.as_bytes())
        .with_context(|| format!("write queue YAML {}", path.display()))?;
    Ok(())
}

pub fn validate_queue(queue: &QueueFile, id_prefix: &str, id_width: usize) -> Result<()> {
    if queue.version != 1 {
        bail!("queue.yaml version must be 1 (got {})", queue.version);
    }
    if id_width == 0 {
        bail!("id_width must be > 0");
    }

    let expected_prefix = normalize_prefix(id_prefix);
    if expected_prefix.is_empty() {
        bail!("id_prefix must be non-empty");
    }

    let mut seen = HashSet::new();
    for (idx, task) in queue.tasks.iter().enumerate() {
        validate_task_required_fields(idx, task)?;
        validate_task_id(idx, &task.id, &expected_prefix, id_width)?;

        let key = task.id.trim().to_string();
        if !seen.insert(key.clone()) {
            bail!("duplicate task id detected: {}", key);
        }
    }

    Ok(())
}

pub fn validate_queue_set(
    active: &QueueFile,
    done: Option<&QueueFile>,
    id_prefix: &str,
    id_width: usize,
) -> Result<()> {
    validate_queue(active, id_prefix, id_width)?;
    if let Some(done) = done {
        validate_queue(done, id_prefix, id_width)?;

        let active_ids: HashSet<&str> = active.tasks.iter().map(|t| t.id.trim()).collect();
        for task in &done.tasks {
            let id = task.id.trim();
            if active_ids.contains(id) {
                bail!("duplicate task id detected across queue and done: {}", id);
            }
        }
    }

    Ok(())
}

pub fn next_id_across(
    active: &QueueFile,
    done: Option<&QueueFile>,
    id_prefix: &str,
    id_width: usize,
) -> Result<String> {
    validate_queue_set(active, done, id_prefix, id_width)?;
    let expected_prefix = normalize_prefix(id_prefix);

    let mut max_value: u32 = 0;
    for (idx, task) in active.tasks.iter().enumerate() {
        let value = validate_task_id(idx, &task.id, &expected_prefix, id_width)?;
        if value > max_value {
            max_value = value;
        }
    }
    if let Some(done) = done {
        for (idx, task) in done.tasks.iter().enumerate() {
            let value = validate_task_id(idx, &task.id, &expected_prefix, id_width)?;
            if value > max_value {
                max_value = value;
            }
        }
    }

    let next_value = max_value.saturating_add(1);
    Ok(format_id(&expected_prefix, next_value, id_width))
}

pub fn archive_done_tasks(
    queue_path: &Path,
    done_path: &Path,
    id_prefix: &str,
    id_width: usize,
) -> Result<ArchiveReport> {
    let (mut active, repaired_active) = load_queue_with_repair(queue_path, id_prefix, id_width)?;
    warn_if_repaired(queue_path, repaired_active);
    let (mut done, repaired_done) =
        load_queue_or_default_with_repair(done_path, id_prefix, id_width)?;
    warn_if_repaired(done_path, repaired_done);

    validate_queue_set(&active, Some(&done), id_prefix, id_width)?;

    let mut moved_ids = Vec::new();
    let mut remaining = Vec::new();

    for task in active.tasks.into_iter() {
        if task.status != TaskStatus::Done {
            remaining.push(task);
            continue;
        }

        let key = task.id.trim().to_string();
        moved_ids.push(key);
        done.tasks.push(task);
    }

    active.tasks = remaining;

    if moved_ids.is_empty() {
        return Ok(ArchiveReport { moved_ids });
    }

    save_queue(done_path, &done)?;
    save_queue(queue_path, &active)?;

    Ok(ArchiveReport { moved_ids })
}

pub fn set_status(
    queue: &mut QueueFile,
    task_id: &str,
    status: TaskStatus,
    now_rfc3339: &str,
    note: Option<&str>,
) -> Result<()> {
    let now = now_rfc3339.trim();
    if now.is_empty() {
        bail!("now timestamp is required");
    }
    OffsetDateTime::parse(now, &Rfc3339).with_context(|| {
        format!(
            "now timestamp must be a valid RFC3339 UTC timestamp (got: {})",
            now
        )
    })?;

    let needle = task_id.trim();
    if needle.is_empty() {
        bail!("task_id is required");
    }

    let task = queue
        .tasks
        .iter_mut()
        .find(|t| t.id.trim() == needle)
        .ok_or_else(|| anyhow!("task not found: {}", needle))?;

    task.status = status;
    task.updated_at = Some(now.to_string());

    match status {
        TaskStatus::Done => {
            task.completed_at = Some(now.to_string());
        }
        TaskStatus::Todo | TaskStatus::Doing => {
            task.completed_at = None;
        }
    }

    if let Some(note) = note {
        let redacted = redaction::redact_text(note);
        let trimmed = redacted.trim();
        if !trimmed.is_empty() {
            task.notes
                .push(sanitize_yaml_text("Note: ", trimmed).to_string());
        }
    }

    Ok(())
}

pub fn find_task<'a>(queue: &'a QueueFile, task_id: &str) -> Option<&'a Task> {
    let needle = task_id.trim();
    if needle.is_empty() {
        return None;
    }
    queue.tasks.iter().find(|task| task.id.trim() == needle)
}

pub fn find_task_across<'a>(
    active: &'a QueueFile,
    done: Option<&'a QueueFile>,
    task_id: &str,
) -> Option<&'a Task> {
    find_task(active, task_id).or_else(|| done.and_then(|d| find_task(d, task_id)))
}

fn normalize_scope(value: &str) -> String {
    value.trim().to_lowercase()
}

fn sanitize_yaml_text(prefix: &str, value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    if let Some(first) = trimmed.chars().next() {
        if first == '`' {
            return format!("{prefix}{trimmed}");
        }
    }
    trimmed.to_string()
}

fn repair_yaml(raw: &str, id_prefix: &str, id_width: usize) -> Option<String> {
    let mut changed = false;
    let mut updated = raw.to_string();

    if let Some(structured) = repair_yaml_structure(&updated) {
        updated = structured;
        changed = true;
    }

    if let Some(list_fix) = repair_yaml_list_fields(&updated) {
        updated = list_fix;
        changed = true;
    }

    if let Some(scalars) = repair_yaml_scalars(&updated) {
        updated = scalars;
        changed = true;
    }

    if let Some(structured) = repair_queue_schema(&updated, id_prefix, id_width) {
        updated = structured;
        changed = true;
    }

    if changed {
        Some(updated)
    } else {
        None
    }
}

fn repair_yaml_structure(raw: &str) -> Option<String> {
    let mut changed = false;
    let mut out = String::new();

    for line in raw.lines() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();

        let mut updated = line.to_string();

        if let Some(rest) = trimmed.strip_prefix("version:") {
            let rest = rest.trim();
            if rest != "1" || indent > 0 {
                updated = "version: 1".to_string();
                changed = true;
            }
        } else if trimmed.starts_with("tasks:") && indent > 0 {
            updated = "tasks:".to_string();
            changed = true;
        }

        out.push_str(&updated);
        out.push('\n');
    }

    if changed {
        Some(out)
    } else {
        None
    }
}

fn repair_yaml_list_fields(raw: &str) -> Option<String> {
    let mut changed = false;
    let mut out = String::new();
    let mut lines = raw.lines().peekable();

    while let Some(line) = lines.next() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            out.push_str(line);
            out.push('\n');
            continue;
        }

        let indent = line.len() - trimmed.len();
        if let Some((key_raw, rest_raw)) = trimmed.split_once(':') {
            let key = key_raw.trim();
            if is_list_field_key(key) {
                let rest = rest_raw.trim_start();
                if rest.is_empty() {
                    let mut block_lines = Vec::new();
                    let mut consumed_any = false;
                    while let Some(next) = lines.peek().copied() {
                        let next_trim = next.trim_start();
                        if next_trim.is_empty() {
                            consumed_any = true;
                            lines.next();
                            block_lines.push(String::new());
                            continue;
                        }
                        let next_indent = next.len() - next_trim.len();
                        if next_indent <= indent {
                            break;
                        }
                        if next_trim.starts_with("- ") {
                            break;
                        }
                        consumed_any = true;
                        lines.next();
                        block_lines.push(next_trim.to_string());
                    }
                    if consumed_any {
                        changed = true;
                        out.push_str(&" ".repeat(indent));
                        out.push_str(key);
                        out.push_str(":\n");
                        out.push_str(&" ".repeat(indent + 2));
                        out.push_str("- |\n");
                        for block in block_lines {
                            out.push_str(&" ".repeat(indent + 4));
                            out.push_str(&block);
                            out.push('\n');
                        }
                        continue;
                    }
                } else if rest.starts_with('|') || rest.starts_with('>') {
                    let indicator = rest.chars().next().unwrap_or('|');
                    let mut block_lines = Vec::new();
                    while let Some(next) = lines.peek().copied() {
                        let next_trim = next.trim_start();
                        if next_trim.is_empty() {
                            lines.next();
                            block_lines.push(String::new());
                            continue;
                        }
                        let next_indent = next.len() - next_trim.len();
                        if next_indent <= indent {
                            break;
                        }
                        lines.next();
                        block_lines.push(next_trim.to_string());
                    }
                    if !block_lines.is_empty() {
                        changed = true;
                        out.push_str(&" ".repeat(indent));
                        out.push_str(key);
                        out.push_str(":\n");
                        out.push_str(&" ".repeat(indent + 2));
                        out.push_str("- ");
                        out.push(indicator);
                        out.push('\n');
                        for block in block_lines {
                            out.push_str(&" ".repeat(indent + 4));
                            out.push_str(&block);
                            out.push('\n');
                        }
                        continue;
                    }
                } else if !rest.starts_with('[') && !rest.starts_with('{') && !rest.starts_with('-')
                {
                    let mut block_lines = vec![rest.trim().to_string()];
                    while let Some(next) = lines.peek().copied() {
                        let next_trim = next.trim_start();
                        if next_trim.is_empty() {
                            lines.next();
                            block_lines.push(String::new());
                            continue;
                        }
                        let next_indent = next.len() - next_trim.len();
                        if next_indent <= indent {
                            break;
                        }
                        if next_trim.starts_with("- ") {
                            break;
                        }
                        lines.next();
                        block_lines.push(next_trim.to_string());
                    }
                    if block_lines.len() > 1 {
                        changed = true;
                        out.push_str(&" ".repeat(indent));
                        out.push_str(key);
                        out.push_str(":\n");
                        out.push_str(&" ".repeat(indent + 2));
                        out.push_str("- |\n");
                        for block in block_lines {
                            out.push_str(&" ".repeat(indent + 4));
                            out.push_str(&block);
                            out.push('\n');
                        }
                        continue;
                    }

                    let mut quoted_changed = false;
                    let quoted = quote_scalar_for_yaml(rest.trim(), &mut quoted_changed);
                    changed = true;
                    out.push_str(&" ".repeat(indent));
                    out.push_str(key);
                    out.push_str(":\n");
                    out.push_str(&" ".repeat(indent + 2));
                    out.push_str("- ");
                    out.push_str(&quoted);
                    out.push('\n');
                    continue;
                }
            }
        }

        out.push_str(line);
        out.push('\n');
    }

    if changed {
        Some(out)
    } else {
        None
    }
}

fn repair_yaml_scalars(raw: &str) -> Option<String> {
    let mut changed = false;
    let mut out = String::new();

    for line in raw.lines() {
        let mut updated = line.to_string();
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            out.push_str(&updated);
            out.push('\n');
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("- ") {
            let indent = line.len() - trimmed.len();
            let is_id = is_id_field(rest);
            let is_field = looks_like_task_start(rest);

            let skip_quote = if indent <= 2 { is_field } else { is_id };

            if !skip_quote && (should_quote_scalar(rest) || looks_like_mapping(rest)) {
                let value = rest.trim();
                updated = format!(
                    "{}- {}",
                    " ".repeat(indent),
                    quote_scalar_for_yaml(value, &mut changed)
                );
            }
        } else if let Some((left, right)) = line.split_once(": ") {
            let key = left.trim();
            if !key.is_empty() && should_quote_scalar(right) {
                let value = right.trim();
                let indent = left.len() - left.trim_start().len();
                updated = format!(
                    "{}{}: {}",
                    " ".repeat(indent),
                    key,
                    quote_scalar_for_yaml(value, &mut changed)
                );
            }
        }

        out.push_str(&updated);
        out.push('\n');
    }

    if changed {
        Some(out)
    } else {
        None
    }
}

fn repair_queue_schema(raw: &str, id_prefix: &str, id_width: usize) -> Option<String> {
    let mut changed = false;
    let mut queue: QueueFile = serde_yaml::from_str(raw).ok()?;

    if queue.version != 1 {
        queue.version = 1;
        changed = true;
    }

    let mut ids = Vec::new();
    let mut id_changed = false;
    for task in queue.tasks.iter_mut() {
        let trimmed = task.id.trim();
        if trimmed.is_empty() {
            continue;
        }
        match parse_any_task_id(trimmed) {
            Some(value) => {
                let prefix = normalize_prefix(id_prefix);
                let normalized = format_id(&prefix, value, id_width);
                if normalized != trimmed {
                    task.id = normalized;
                    id_changed = true;
                }
                ids.push(value);
            }
            None => continue,
        }
    }

    let mut next_value = if ids.is_empty() {
        1
    } else {
        ids.into_iter().max().unwrap_or(0).saturating_add(1)
    };

    for task in queue.tasks.iter_mut() {
        if task.id.trim().is_empty() {
            let prefix = normalize_prefix(id_prefix);
            task.id = format_id(&prefix, next_value, id_width);
            next_value = next_value.saturating_add(1);
            id_changed = true;
        }

        let trimmed = task.id.trim();
        if let Some(value) = parse_any_task_id(trimmed) {
            let prefix = normalize_prefix(id_prefix);
            let normalized = format_id(&prefix, value, id_width);
            if normalized != trimmed {
                task.id = normalized;
                id_changed = true;
            }
        }
    }

    if id_changed {
        ensure_unique_task_ids(&mut queue, id_prefix, id_width);
        changed = true;
    }

    let now = timeutil::now_utc_rfc3339().ok();
    if normalize_task_fields(&mut queue, now.as_deref()) {
        changed = true;
    }

    if changed {
        serde_yaml::to_string(&queue).ok()
    } else {
        None
    }
}

fn normalize_task_fields(queue: &mut QueueFile, now: Option<&str>) -> bool {
    let mut changed = false;

    for task in queue.tasks.iter_mut() {
        if task.title.trim().is_empty() {
            task.title = "Untitled task".to_string();
            changed = true;
        } else if task.title != task.title.trim() {
            task.title = task.title.trim().to_string();
            changed = true;
        }

        changed |= normalize_string_list(&mut task.tags, "unspecified");
        changed |= normalize_string_list(&mut task.scope, "unspecified");
        changed |= normalize_string_list(&mut task.evidence, "pending evidence");
        changed |= normalize_string_list(&mut task.plan, "pending plan");
        changed |= normalize_string_list(&mut task.notes, "");

        if task.request.as_ref().is_none_or(|r| r.trim().is_empty()) {
            task.request = Some("scan repair".to_string());
            changed = true;
        } else if let Some(value) = task.request.as_ref() {
            let trimmed = value.trim();
            if trimmed != value {
                task.request = Some(trimmed.to_string());
                changed = true;
            }
        }

        if normalize_optional_timestamp(&mut task.created_at, now) {
            changed = true;
        }
        if normalize_optional_timestamp(&mut task.updated_at, now) {
            changed = true;
        }
        if normalize_optional_timestamp(&mut task.completed_at, None) {
            changed = true;
        }
    }

    changed
}

fn normalize_string_list(values: &mut Vec<String>, fallback: &str) -> bool {
    let mut changed = false;
    let mut normalized = Vec::new();
    for value in values.iter() {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            changed = true;
            continue;
        }
        if trimmed != value {
            changed = true;
        }
        normalized.push(trimmed.to_string());
    }

    if *values != normalized {
        *values = normalized;
        changed = true;
    }

    if values.is_empty() && !fallback.is_empty() {
        values.push(fallback.to_string());
        changed = true;
    }

    changed
}

fn normalize_optional_timestamp(value: &mut Option<String>, fallback: Option<&str>) -> bool {
    let Some(current) = value.as_ref() else {
        if let Some(fallback) = fallback {
            if !fallback.trim().is_empty() {
                *value = Some(fallback.trim().to_string());
                return true;
            }
        }
        return false;
    };
    let trimmed = current.trim();
    if trimmed.is_empty() {
        if let Some(fallback) = fallback {
            if !fallback.trim().is_empty() {
                *value = Some(fallback.trim().to_string());
                return true;
            }
        }
        *value = None;
        return true;
    }

    if trimmed == current {
        return false;
    }

    *value = Some(trimmed.to_string());
    true
}

fn parse_any_task_id(raw: &str) -> Option<u32> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some((_, suffix)) = trimmed.split_once('-') {
        return suffix.trim().parse::<u32>().ok();
    }

    trimmed.parse::<u32>().ok()
}

fn ensure_unique_task_ids(queue: &mut QueueFile, id_prefix: &str, id_width: usize) {
    let expected_prefix = normalize_prefix(id_prefix);
    let mut seen = HashSet::new();
    let mut max_value = 0u32;

    for task in queue.tasks.iter() {
        if let Some(value) = parse_any_task_id(task.id.trim()) {
            max_value = max_value.max(value);
        }
    }

    for task in queue.tasks.iter_mut() {
        let trimmed = task.id.trim().to_string();
        if !seen.insert(trimmed.clone()) {
            max_value = max_value.saturating_add(1);
            task.id = format_id(&expected_prefix, max_value, id_width);
        }
    }
}

fn is_task_field_key(key: &str) -> bool {
    matches!(
        key,
        "id" | "status"
            | "title"
            | "tags"
            | "scope"
            | "evidence"
            | "plan"
            | "notes"
            | "request"
            | "agent"
            | "created_at"
            | "updated_at"
            | "completed_at"
    )
}

fn is_list_field_key(key: &str) -> bool {
    matches!(key, "tags" | "scope" | "evidence" | "plan" | "notes")
}

fn is_id_field(value: &str) -> bool {
    if let Some((left, _)) = value.split_once(": ") {
        return left.trim() == "id";
    }
    if let Some(left) = value.strip_suffix(':') {
        return left.trim() == "id";
    }
    false
}

fn looks_like_task_start(value: &str) -> bool {
    if let Some((left, _)) = value.split_once(": ") {
        return is_task_field_key(left.trim());
    }
    if let Some(left) = value.strip_suffix(':') {
        return is_task_field_key(left.trim());
    }
    false
}

fn should_quote_scalar(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.starts_with('\'')
        || trimmed.starts_with('"')
        || trimmed.starts_with('{')
        || trimmed.starts_with('[')
        || trimmed.starts_with('|')
        || trimmed.starts_with('>')
    {
        return false;
    }
    has_colon_needing_quote(trimmed)
}

fn has_colon_needing_quote(value: &str) -> bool {
    let mut chars = value.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == ':' {
            match chars.peek() {
                Some(next) if next.is_whitespace() => return true,
                None => return true,
                _ => {}
            }
        }
    }
    false
}

fn looks_like_mapping(value: &str) -> bool {
    let trimmed = value.trim_start();
    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphanumeric() {
        return false;
    }
    for ch in chars {
        if ch == ':' {
            return true;
        }
        if !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '-') {
            return false;
        }
    }
    false
}

fn escape_single_quotes(value: &str) -> String {
    let normalized = value.replace("\\t", "\t");
    normalized.replace('\'', "''")
}

fn quote_scalar_for_yaml(value: &str, changed: &mut bool) -> String {
    if should_use_double_quotes(value) {
        let escaped = escape_double_quotes(value);
        *changed = true;
        format!("\"{}\"", escaped)
    } else {
        let escaped = escape_single_quotes(value);
        *changed = true;
        format!("'{}'", escaped)
    }
}

fn should_use_double_quotes(value: &str) -> bool {
    value.contains("\\n")
        || value.contains("\\r")
        || value.contains("\\u")
        || value.contains("\\U")
        || value.contains("\\x")
}

fn escape_double_quotes(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => {
                if let Some(next) = chars.peek().copied() {
                    if is_allowed_yaml_escape(next) {
                        out.push('\\');
                        out.push(next);
                        chars.next();
                    } else {
                        out.push_str("\\\\");
                    }
                } else {
                    out.push_str("\\\\");
                }
            }
            _ => out.push(ch),
        }
    }

    out
}

fn is_allowed_yaml_escape(ch: char) -> bool {
    matches!(
        ch,
        '0' | 'a'
            | 'b'
            | 't'
            | 'n'
            | 'v'
            | 'f'
            | 'r'
            | 'e'
            | '"'
            | '\\'
            | 'N'
            | '_'
            | 'L'
            | 'P'
            | 'x'
            | 'u'
            | 'U'
    )
}

pub fn next_todo_task(queue: &QueueFile) -> Option<&Task> {
    queue
        .tasks
        .iter()
        .find(|task| task.status == TaskStatus::Todo)
}

pub fn backfill_missing_fields(
    queue: &mut QueueFile,
    new_task_ids: &[String],
    default_request: &str,
    now_utc: &str,
) {
    let now = now_utc.trim();
    if now.is_empty() {
        return;
    }

    for task in queue.tasks.iter_mut() {
        if !new_task_ids.contains(&task.id.trim().to_string()) {
            continue;
        }

        if task.request.as_ref().is_none_or(|r| r.trim().is_empty()) {
            let req = default_request.trim();
            if !req.is_empty() {
                task.request = Some(req.to_string());
            }
        }

        if task.created_at.as_ref().is_none_or(|t| t.trim().is_empty()) {
            task.created_at = Some(now.to_string());
        }

        if task.updated_at.as_ref().is_none_or(|t| t.trim().is_empty()) {
            task.updated_at = Some(now.to_string());
        }
    }
}

pub fn filter_tasks<'a>(
    queue: &'a QueueFile,
    statuses: &[TaskStatus],
    tags: &[String],
    scopes: &[String],
    limit: Option<usize>,
) -> Vec<&'a Task> {
    let status_filter: HashSet<TaskStatus> = statuses.iter().copied().collect();
    let tag_filter: HashSet<String> = tags
        .iter()
        .map(|tag| normalize_tag(tag))
        .filter(|tag| !tag.is_empty())
        .collect();
    let scope_filter: Vec<String> = scopes
        .iter()
        .map(|scope| normalize_scope(scope))
        .filter(|scope| !scope.is_empty())
        .collect();

    let has_status_filter = !status_filter.is_empty();
    let has_tag_filter = !tag_filter.is_empty();
    let has_scope_filter = !scope_filter.is_empty();

    let mut out = Vec::new();
    for task in &queue.tasks {
        if has_status_filter && !status_filter.contains(&task.status) {
            continue;
        }
        if has_tag_filter
            && !task
                .tags
                .iter()
                .any(|tag| tag_filter.contains(&normalize_tag(tag)))
        {
            continue;
        }
        if has_scope_filter
            && !task.scope.iter().any(|scope| {
                let hay = normalize_scope(scope);
                scope_filter.iter().any(|needle| hay.contains(needle))
            })
        {
            continue;
        }

        out.push(task);
        if let Some(limit) = limit {
            if out.len() >= limit {
                break;
            }
        }
    }
    out
}

fn normalize_prefix(prefix: &str) -> String {
    prefix.trim().to_uppercase()
}

fn normalize_tag(tag: &str) -> String {
    tag.trim().to_lowercase()
}

fn validate_task_required_fields(index: usize, task: &Task) -> Result<()> {
    if task.id.trim().is_empty() {
        bail!("task[{}] id is required", index);
    }
    if task.title.trim().is_empty() {
        bail!("task[{}] title is required (id={})", index, task.id);
    }
    ensure_list_non_empty("tags", index, &task.id, &task.tags)?;
    ensure_list_non_empty("scope", index, &task.id, &task.scope)?;
    ensure_list_non_empty("evidence", index, &task.id, &task.evidence)?;
    ensure_list_non_empty("plan", index, &task.id, &task.plan)?;
    ensure_field_present("request", index, &task.id, task.request.as_deref())?;

    if let Some(ts) = task.created_at.as_deref() {
        validate_rfc3339("created_at", index, &task.id, ts)?;
    } else {
        bail!("task[{}] created_at is required (id={})", index, task.id);
    }

    if let Some(ts) = task.updated_at.as_deref() {
        validate_rfc3339("updated_at", index, &task.id, ts)?;
    } else {
        bail!("task[{}] updated_at is required (id={})", index, task.id);
    }

    if let Some(ts) = task.completed_at.as_deref() {
        validate_rfc3339("completed_at", index, &task.id, ts)?;
    }

    Ok(())
}

fn validate_rfc3339(label: &str, index: usize, id: &str, value: &str) -> Result<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!(
            "task[{}] {} is required and must be non-empty (id={})",
            index,
            label,
            id
        );
    }
    OffsetDateTime::parse(trimmed, &Rfc3339).with_context(|| {
        format!(
            "task[{}] {} must be a valid RFC3339 UTC timestamp (got: {}, id={})",
            index, label, trimmed, id
        )
    })?;
    Ok(())
}

fn ensure_list_non_empty(label: &str, index: usize, id: &str, values: &[String]) -> Result<()> {
    if values.is_empty() {
        bail!("task[{}] {} must be non-empty (id={})", index, label, id);
    }
    for (i, value) in values.iter().enumerate() {
        if value.trim().is_empty() {
            bail!(
                "task[{}] {}[{}] must be non-empty (id={})",
                index,
                label,
                i,
                id
            );
        }
    }
    Ok(())
}

fn ensure_field_present(label: &str, index: usize, id: &str, value: Option<&str>) -> Result<()> {
    match value {
        Some(v) if !v.trim().is_empty() => Ok(()),
        _ => bail!(
            "task[{}] {} is required and must be non-empty (id={})",
            index,
            label,
            id
        ),
    }
}

fn validate_task_id(
    index: usize,
    raw_id: &str,
    expected_prefix: &str,
    id_width: usize,
) -> Result<u32> {
    let trimmed = raw_id.trim();
    let (prefix_raw, num_raw) = trimmed
        .split_once('-')
        .ok_or_else(|| anyhow!("task[{}] id must contain '-' (got: {})", index, trimmed))?;

    let prefix = prefix_raw.trim().to_uppercase();
    if prefix != expected_prefix {
        bail!(
            "task[{}] id prefix must be {} (got: {})",
            index,
            expected_prefix,
            prefix
        );
    }

    let num = num_raw.trim();
    if num.len() != id_width {
        bail!(
            "task[{}] id numeric width must be {} digits (got: {})",
            index,
            id_width,
            num
        );
    }
    if !num.chars().all(|c| c.is_ascii_digit()) {
        bail!(
            "task[{}] id numeric suffix must be digits (got: {})",
            index,
            num
        );
    }

    let value: u32 = num.parse().with_context(|| {
        format!(
            "task[{}] id numeric suffix must parse as integer (got: {})",
            index, num
        )
    })?;
    Ok(value)
}

fn format_id(prefix: &str, number: u32, width: usize) -> String {
    format!("{}-{:0width$}", prefix, number, width = width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{Task, TaskStatus};
    use tempfile::TempDir;

    fn task(id: &str) -> Task {
        task_with(id, TaskStatus::Todo, vec!["code".to_string()])
    }

    fn task_with(id: &str, status: TaskStatus, tags: Vec<String>) -> Task {
        Task {
            id: id.to_string(),
            status,
            title: "Test task".to_string(),
            tags,
            scope: vec!["crates/ralph".to_string()],
            evidence: vec!["observed".to_string()],
            plan: vec!["do thing".to_string()],
            notes: vec![],
            request: Some("test request".to_string()),
            agent: None,
            created_at: Some("2026-01-18T00:00:00Z".to_string()),
            updated_at: Some("2026-01-18T00:00:00Z".to_string()),
            completed_at: None,
        }
    }

    #[test]
    fn validate_rejects_duplicate_ids() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001"), task("RQ-0001")],
        };
        let err = validate_queue(&queue, "RQ", 4).unwrap_err();
        let msg = format!("{err:#}");
        assert!(
            msg.to_lowercase().contains("duplicate"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn validate_rejects_missing_request() {
        let mut task = task("RQ-0001");
        task.request = None;
        let queue = QueueFile {
            version: 1,
            tasks: vec![task],
        };
        let err = validate_queue(&queue, "RQ", 4).unwrap_err();
        assert!(format!("{err}").contains("request is required"));
    }

    #[test]
    fn validate_rejects_empty_request() {
        let mut task = task("RQ-0001");
        task.request = Some("".to_string());
        let queue = QueueFile {
            version: 1,
            tasks: vec![task],
        };
        let err = validate_queue(&queue, "RQ", 4).unwrap_err();
        assert!(format!("{err}").contains("request is required"));
    }

    #[test]
    fn validate_rejects_missing_created_at() {
        let mut task = task("RQ-0001");
        task.created_at = None;
        let queue = QueueFile {
            version: 1,
            tasks: vec![task],
        };
        let err = validate_queue(&queue, "RQ", 4).unwrap_err();
        assert!(format!("{err}").contains("created_at is required"));
    }

    #[test]
    fn validate_rejects_missing_updated_at() {
        let mut task = task("RQ-0001");
        task.updated_at = None;
        let queue = QueueFile {
            version: 1,
            tasks: vec![task],
        };
        let err = validate_queue(&queue, "RQ", 4).unwrap_err();
        assert!(format!("{err}").contains("updated_at is required"));
    }

    #[test]
    fn validate_rejects_invalid_rfc3339() {
        let mut task = task("RQ-0001");
        task.created_at = Some("not a date".to_string());
        let queue = QueueFile {
            version: 1,
            tasks: vec![task],
        };
        let err = validate_queue(&queue, "RQ", 4).unwrap_err();
        assert!(format!("{err}").contains("must be a valid RFC3339 UTC timestamp"));
    }

    #[test]
    fn set_status_rejects_invalid_rfc3339() -> Result<()> {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };

        let err =
            set_status(&mut queue, "RQ-0001", TaskStatus::Doing, "invalid", None).unwrap_err();
        assert!(format!("{err}").contains("must be a valid RFC3339 UTC timestamp"));
        Ok(())
    }

    #[test]
    fn set_status_updates_timestamps_and_fields() -> Result<()> {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };

        let now = "2026-01-17T00:00:00Z";
        set_status(
            &mut queue,
            "RQ-0001",
            TaskStatus::Doing,
            now,
            Some("started"),
        )?;
        let t = &queue.tasks[0];
        assert_eq!(t.status, TaskStatus::Doing);
        assert_eq!(t.updated_at.as_deref(), Some(now));
        assert_eq!(t.completed_at, None);
        assert_eq!(t.notes, vec!["started".to_string()]);

        let now2 = "2026-01-17T00:02:00Z";
        set_status(
            &mut queue,
            "RQ-0001",
            TaskStatus::Done,
            now2,
            Some("completed"),
        )?;
        let t = &queue.tasks[0];
        assert_eq!(t.status, TaskStatus::Done);
        assert_eq!(t.updated_at.as_deref(), Some(now2));
        assert_eq!(t.completed_at.as_deref(), Some(now2));
        assert!(t.notes.iter().any(|n| n == "completed"));

        Ok(())
    }

    #[test]
    fn set_status_redacts_note() -> Result<()> {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };

        let now = "2026-01-17T00:00:00Z";
        set_status(
            &mut queue,
            "RQ-0001",
            TaskStatus::Doing,
            now,
            Some("API_KEY=abc12345"),
        )?;

        let t = &queue.tasks[0];
        assert_eq!(t.notes, vec!["API_KEY=[REDACTED]".to_string()]);

        Ok(())
    }

    #[test]
    fn set_status_sanitizes_leading_backticks() -> Result<()> {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };

        let now = "2026-01-17T00:00:00Z";
        set_status(
            &mut queue,
            "RQ-0001",
            TaskStatus::Doing,
            now,
            Some("`make ci` failed"),
        )?;

        let t = &queue.tasks[0];
        assert_eq!(t.notes, vec!["Note: `make ci` failed".to_string()]);

        Ok(())
    }

    #[test]
    fn validate_queue_set_rejects_cross_file_duplicates() {
        let active = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };
        let done = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };
        let err = validate_queue_set(&active, Some(&done), "RQ", 4).unwrap_err();
        assert!(format!("{err}").contains("duplicate task id detected across queue and done"));
    }

    #[test]
    fn next_id_across_includes_done() -> Result<()> {
        let active = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0002")],
        };
        let done = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0009")],
        };
        let next = next_id_across(&active, Some(&done), "RQ", 4)?;
        assert_eq!(next, "RQ-0010");
        Ok(())
    }

    #[test]
    fn load_queue_with_repair_quotes_colon_scalars() -> Result<()> {
        let dir = TempDir::new()?;
        let queue_path = dir.path().join("queue.yaml");

        let raw = r#"version: 1
tasks:
  - id: RQ-0001
    status: todo
    title: Normalize empty queue YAML and handle tasks: null safely
    tags:
      - queue
    scope:
      - crates/ralph/src/queue.rs
    evidence:
      - queues can break: null tasks
    plan:
      - Fix parsing: add a safe default
    notes:
      - key: value
      - trailing:
      - tab:\tvalue
    request: test
    created_at: 2026-01-18T00:00:00Z
    updated_at: 2026-01-18T00:00:00Z
"#;
        std::fs::write(&queue_path, raw)?;

        let (queue, repaired) = load_queue_with_repair(&queue_path, "RQ", 4)?;
        assert!(repaired);
        assert_eq!(
            queue.tasks[0].title,
            "Normalize empty queue YAML and handle tasks: null safely"
        );
        assert_eq!(
            queue.tasks[0].notes,
            vec![
                "key: value".to_string(),
                "trailing:".to_string(),
                "tab:\tvalue".to_string()
            ]
        );

        let repaired_raw = std::fs::read_to_string(&queue_path)?;
        assert!(repaired_raw
            .contains("title: 'Normalize empty queue YAML and handle tasks: null safely'"));
        assert!(repaired_raw.contains("- 'queues can break: null tasks'"));
        assert!(repaired_raw.contains("- 'Fix parsing: add a safe default'"));
        assert!(repaired_raw.contains("- 'key: value'"));
        assert!(repaired_raw.contains("- 'trailing:'"));
        assert!(repaired_raw.contains("- 'tab:\tvalue'"));
        Ok(())
    }

    #[test]
    fn load_queue_with_repair_fixes_indented_tasks_key() -> Result<()> {
        let dir = TempDir::new()?;
        let queue_path = dir.path().join("queue.yaml");

        let raw = r#"version: 1
  tasks:
  - id: RQ-0001
    status: todo
    title: Fix indentation
    tags:
      - rust
    scope:
      - crates/ralph
    evidence:
      - regression test
    plan:
      - repair indentation
    request: test
    created_at: 2026-01-18T00:00:00Z
    updated_at: 2026-01-18T00:00:00Z
"#;
        std::fs::write(&queue_path, raw)?;

        let (queue, repaired) = load_queue_with_repair(&queue_path, "RQ", 4)?;
        assert!(repaired);
        assert_eq!(queue.tasks.len(), 1);

        let fixed = std::fs::read_to_string(&queue_path)?;
        assert!(fixed.contains("\ntasks:\n"));

        Ok(())
    }

    #[test]
    fn repair_queue_repairs_invalid_yaml() -> Result<()> {
        let dir = TempDir::new()?;
        let queue_path = dir.path().join("queue.yaml");

        let raw = r#"version: 1
tasks:
  - id: RQ-0001
    status: todo
    title: Fix colon: in this title
    tags:
      - test
    scope:
      - file:rs
    evidence:
      - contains colon: in evidence
    plan:
      - Repair YAML scalars with colons
      - Test queue repair behavior
    notes:
      - map: value
      - trailing:
    request: test
    created_at: 2026-01-18T00:00:00Z
    updated_at: 2026-01-18T00:00:00Z
"#;
        std::fs::write(&queue_path, raw)?;

        let report = repair_queue(&queue_path, "RQ", 4)?;
        assert!(report.repaired, "repair should have occurred");

        let (queue, repaired) = load_queue_with_repair(&queue_path, "RQ", 4)?;
        assert!(!repaired, "repaired queue should parse cleanly");
        assert_eq!(queue.tasks.len(), 1);
        assert_eq!(queue.tasks[0].id, "RQ-0001");
        assert_eq!(queue.tasks[0].title, "Fix colon: in this title");

        let file_content = std::fs::read_to_string(&queue_path)?;
        assert!(
            file_content.contains("title: 'Fix colon: in this title'")
                || file_content.contains("title: \"Fix colon: in this title\""),
            "file on disk should have quoted colon scalar"
        );
        assert!(
            file_content.contains("- 'contains colon: in evidence'")
                || file_content.contains("- \"contains colon: in evidence\""),
            "evidence list item with colon should be quoted"
        );
        assert!(file_content.contains("- 'map: value'"));
        assert!(file_content.contains("- 'trailing:'"));
        Ok(())
    }

    #[test]
    fn repair_queue_returns_false_for_valid_yaml() -> Result<()> {
        let dir = TempDir::new()?;
        let queue_path = dir.path().join("queue.yaml");

        let queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };
        save_queue(&queue_path, &queue)?;

        let report = repair_queue(&queue_path, "RQ", 4)?;
        assert!(
            !report.repaired,
            "repair should not have occurred for valid YAML"
        );

        let (queue_after, repaired) = load_queue_with_repair(&queue_path, "RQ", 4)?;
        assert!(!repaired, "expected valid queue to parse cleanly");
        assert_eq!(queue_after.tasks.len(), 1);
        assert_eq!(queue_after.tasks[0].id, "RQ-0001");
        Ok(())
    }

    #[test]
    fn archive_done_tasks_fails_on_duplicates() -> Result<()> {
        let dir = TempDir::new()?;
        let queue_path = dir.path().join("queue.yaml");
        let done_path = dir.path().join("done.yaml");

        let mut done_task = task("RQ-0001");
        done_task.status = TaskStatus::Done;

        let mut active_task = task("RQ-0001");
        active_task.status = TaskStatus::Done;

        let active = QueueFile {
            version: 1,
            tasks: vec![active_task],
        };

        let done = QueueFile {
            version: 1,
            tasks: vec![done_task],
        };

        save_queue(&queue_path, &active)?;
        save_queue(&done_path, &done)?;

        let err = archive_done_tasks(&queue_path, &done_path, "RQ", 4).unwrap_err();
        assert!(format!("{err}").contains("duplicate task id detected across queue and done"));

        Ok(())
    }

    #[test]
    fn archive_done_tasks_moves_and_dedupes() -> Result<()> {
        let dir = TempDir::new()?;
        let queue_path = dir.path().join("queue.yaml");
        let done_path = dir.path().join("done.yaml");

        let mut done_task = task("RQ-0002");
        done_task.status = TaskStatus::Done;

        let mut active_task = task("RQ-0001");
        active_task.status = TaskStatus::Done;

        let mut active_task_two = task("RQ-0003");
        active_task_two.status = TaskStatus::Done;

        let active = QueueFile {
            version: 1,
            tasks: vec![active_task, active_task_two],
        };

        let done = QueueFile {
            version: 1,
            tasks: vec![done_task],
        };

        save_queue(&queue_path, &active)?;
        save_queue(&done_path, &done)?;

        let report = archive_done_tasks(&queue_path, &done_path, "RQ", 4)?;
        assert_eq!(
            report.moved_ids,
            vec!["RQ-0001".to_string(), "RQ-0003".to_string()]
        );

        let (active_after, repaired_active) = load_queue_with_repair(&queue_path, "RQ", 4)?;
        assert!(
            !repaired_active,
            "expected valid active queue after archive"
        );
        assert!(active_after.tasks.is_empty());

        let (done_after, repaired_done) = load_queue_with_repair(&done_path, "RQ", 4)?;
        assert!(!repaired_done, "expected valid done queue after archive");
        assert_eq!(done_after.tasks.len(), 3);

        let report2 = archive_done_tasks(&queue_path, &done_path, "RQ", 4)?;
        assert!(report2.moved_ids.is_empty());
        Ok(())
    }

    #[test]
    fn find_task_returns_none_for_missing_or_blank() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };
        assert!(find_task(&queue, "").is_none());
        assert!(find_task(&queue, "RQ-9999").is_none());
    }

    #[test]
    fn next_todo_task_picks_first_todo() {
        let mut todo = task("RQ-0002");
        todo.status = TaskStatus::Todo;
        let mut doing = task("RQ-0001");
        doing.status = TaskStatus::Doing;

        let queue = QueueFile {
            version: 1,
            tasks: vec![doing, todo],
        };
        let found = next_todo_task(&queue).expect("todo task");
        assert_eq!(found.id, "RQ-0002");
    }

    #[test]
    fn next_todo_task_none_when_empty() {
        let mut doing = task("RQ-0001");
        doing.status = TaskStatus::Doing;
        let queue = QueueFile {
            version: 1,
            tasks: vec![doing],
        };
        assert!(next_todo_task(&queue).is_none());
    }

    #[test]
    fn filter_tasks_by_status_and_tag() {
        let mut todo = task_with(
            "RQ-0001",
            TaskStatus::Todo,
            vec!["rust".to_string(), "queue".to_string()],
        );
        todo.title = "First".to_string();
        let doing = task_with("RQ-0002", TaskStatus::Doing, vec!["docs".to_string()]);
        let done = task_with("RQ-0003", TaskStatus::Done, vec!["RUST".to_string()]);

        let queue = QueueFile {
            version: 1,
            tasks: vec![todo, doing, done],
        };

        let filtered = filter_tasks(
            &queue,
            &[TaskStatus::Todo, TaskStatus::Done],
            &["rust".to_string()],
            &[],
            None,
        );
        let ids: Vec<&str> = filtered.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["RQ-0001", "RQ-0003"]);
    }

    #[test]
    fn filter_tasks_applies_limit() {
        let queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001"), task("RQ-0002"), task("RQ-0003")],
        };

        let filtered = filter_tasks(&queue, &[], &[], &[], Some(2));
        let ids: Vec<&str> = filtered.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["RQ-0001", "RQ-0002"]);
    }

    #[test]
    fn filter_tasks_by_scope_is_case_insensitive_substring() {
        let mut a = task("RQ-0001");
        a.scope = vec![
            "crates/ralph/src/main.rs".to_string(),
            "make ci".to_string(),
        ];
        let mut b = task("RQ-0002");
        b.scope = vec!["docs/README.md".to_string()];

        let queue = QueueFile {
            version: 1,
            tasks: vec![a, b],
        };

        let filtered = filter_tasks(&queue, &[], &[], &["MAIN.RS".to_string()], None);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, "RQ-0001");
    }

    #[test]
    fn find_task_across_prefers_active_then_done() {
        let active = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };
        let done = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0002")],
        };

        let found_active = find_task_across(&active, Some(&done), "RQ-0001").expect("active");
        assert_eq!(found_active.id, "RQ-0001");

        let found_done = find_task_across(&active, Some(&done), "RQ-0002").expect("done");
        assert_eq!(found_done.id, "RQ-0002");

        assert!(find_task_across(&active, Some(&done), "RQ-9999").is_none());
    }

    #[test]
    fn load_queue_or_default_with_repair_returns_default_when_missing() -> Result<()> {
        let dir = TempDir::new()?;
        let queue_path = dir.path().join("missing.yaml");
        let (queue, repaired) = load_queue_or_default_with_repair(&queue_path, "RQ", 4)?;
        assert!(!repaired);
        assert!(queue.tasks.is_empty());
        Ok(())
    }

    #[test]
    fn backfill_missing_fields_populates_request() -> Result<()> {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };
        queue.tasks[0].request = None;

        backfill_missing_fields(
            &mut queue,
            &["RQ-0001".to_string()],
            "default request",
            "2026-01-18T00:00:00Z",
        );

        assert_eq!(queue.tasks[0].request, Some("default request".to_string()));
        Ok(())
    }

    #[test]
    fn backfill_missing_fields_populates_timestamps() -> Result<()> {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };
        queue.tasks[0].created_at = None;
        queue.tasks[0].updated_at = None;

        backfill_missing_fields(
            &mut queue,
            &["RQ-0001".to_string()],
            "default request",
            "2026-01-18T12:34:56Z",
        );

        assert_eq!(
            queue.tasks[0].created_at,
            Some("2026-01-18T12:34:56Z".to_string())
        );
        assert_eq!(
            queue.tasks[0].updated_at,
            Some("2026-01-18T12:34:56Z".to_string())
        );
        Ok(())
    }

    #[test]
    fn backfill_missing_fields_skips_existing_values() -> Result<()> {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };

        backfill_missing_fields(
            &mut queue,
            &["RQ-0001".to_string()],
            "new request",
            "2026-01-18T12:34:56Z",
        );

        assert_eq!(queue.tasks[0].request, Some("test request".to_string()));
        assert_eq!(
            queue.tasks[0].created_at,
            Some("2026-01-18T00:00:00Z".to_string())
        );
        assert_eq!(
            queue.tasks[0].updated_at,
            Some("2026-01-18T00:00:00Z".to_string())
        );
        Ok(())
    }

    #[test]
    fn backfill_missing_fields_only_affects_specified_ids() -> Result<()> {
        let mut t1 = task("RQ-0001");
        t1.request = None;
        let t2 = task("RQ-0002");
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![t1, t2],
        };

        backfill_missing_fields(
            &mut queue,
            &["RQ-0001".to_string()],
            "backfilled request",
            "2026-01-18T12:34:56Z",
        );

        assert_eq!(
            queue.tasks[0].request,
            Some("backfilled request".to_string())
        );
        assert_eq!(queue.tasks[1].request, Some("test request".to_string()));
        Ok(())
    }

    #[test]
    fn backfill_missing_fields_handles_empty_string_as_missing() -> Result<()> {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };
        queue.tasks[0].request = Some("".to_string());
        queue.tasks[0].created_at = Some("".to_string());
        queue.tasks[0].updated_at = Some("".to_string());

        backfill_missing_fields(
            &mut queue,
            &["RQ-0001".to_string()],
            "default request",
            "2026-01-18T12:34:56Z",
        );

        assert_eq!(queue.tasks[0].request, Some("default request".to_string()));
        assert_eq!(
            queue.tasks[0].created_at,
            Some("2026-01-18T12:34:56Z".to_string())
        );
        assert_eq!(
            queue.tasks[0].updated_at,
            Some("2026-01-18T12:34:56Z".to_string())
        );
        Ok(())
    }

    #[test]
    fn backfill_missing_fields_empty_now_skips() -> Result<()> {
        let mut queue = QueueFile {
            version: 1,
            tasks: vec![task("RQ-0001")],
        };
        queue.tasks[0].created_at = None;
        queue.tasks[0].updated_at = None;

        backfill_missing_fields(&mut queue, &["RQ-0001".to_string()], "default request", "");

        assert_eq!(queue.tasks[0].created_at, None);
        assert_eq!(queue.tasks[0].updated_at, None);
        Ok(())
    }
}
