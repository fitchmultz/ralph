//! Unix-only fake `gh` binaries for queue issue tests.
//!
//! Responsibilities:
//! - Generate executable fake `gh` shims for issue publish tests.
//! - Simulate auth success/failure and issue create/edit flows without network access.
//! - Keep shell-script behavior centralized so test files stay focused on assertions.
//!
//! Not handled here:
//! - Non-Unix fake CLI support.
//! - Queue fixture construction or test assertions.
//! - Production GitHub CLI integration behavior.
//!
//! Invariants/assumptions:
//! - Callers invoke these helpers only from `#[cfg(unix)]` tests.
//! - Generated scripts are executable and placed ahead of the real PATH.
//! - Script behavior mirrors only the branches exercised by this suite.

#[cfg(unix)]
use std::io::Write;
#[cfg(unix)]
use tempfile::TempDir;

#[cfg(unix)]
fn write_fake_gh_script(tmp_dir: &TempDir, script: &str) -> std::path::PathBuf {
    let bin_dir = tmp_dir.path().join("bin");
    std::fs::create_dir_all(&bin_dir).unwrap();

    let gh_path = bin_dir.join("gh");
    let mut file = std::fs::File::create(&gh_path).unwrap();
    file.write_all(script.as_bytes()).unwrap();

    {
        use std::os::unix::fs::PermissionsExt;

        let mut perms = file.metadata().unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&gh_path, perms).unwrap();
    }

    bin_dir
}

#[cfg(unix)]
fn auth_status_block(auth_ok: bool) -> &'static str {
    if auth_ok {
        "exit 0"
    } else {
        "echo \"You are not logged into any GitHub hosts\" >&2\nexit 1"
    }
}

#[cfg(unix)]
pub(super) fn create_fake_gh_for_issue_publish(
    tmp_dir: &TempDir,
    task_id: &str,
    issue_url: &str,
    auth_ok: bool,
) -> std::path::PathBuf {
    let log_path = tmp_dir.path().join("gh.log");
    let auth_status_block = auth_status_block(auth_ok);

    let script = format!(
        r#"#!/bin/sh
set -eu

LOG="{log_path}"
TASK_ID="{task_id}"
ISSUE_URL="{issue_url}"

if [ "${{1:-}}" = "--version" ]; then
  echo "gh version 0.0.0-test"
  exit 0
fi

if [ "${{1:-}}" = "auth" ] && [ "${{2:-}}" = "status" ]; then
  {auth_status_block}
fi

if [ "${{1:-}}" = "issue" ] && [ "${{2:-}}" = "create" ]; then
  printf '%s\n' "$*" >> "$LOG"

  BODY_FILE=""
  TITLE=""
  i=1
  while [ $i -le $# ]; do
    eval arg=\${{${{i}}}}
    if [ "$arg" = "--body-file" ]; then
      i=$((i+1)); eval BODY_FILE=\${{${{i}}}}
    elif [ "$arg" = "--title" ]; then
      i=$((i+1)); eval TITLE=\${{${{i}}}}
    fi
    i=$((i+1))
  done

  if [ -z "$BODY_FILE" ] || [ ! -f "$BODY_FILE" ]; then
    echo "missing body file" >&2
    exit 2
  fi

  if [ -z "$TITLE" ]; then
    echo "missing title" >&2
    exit 3
  fi

  FOUND=0
  while IFS= read -r line; do
    if [ "$line" = "<!-- ralph_task_id: $TASK_ID -->" ]; then
      FOUND=1
    fi
  done < "$BODY_FILE"

  if [ "$FOUND" -ne 1 ]; then
    echo "missing marker" >&2
    exit 4
  fi

  echo "$ISSUE_URL"
  exit 0
fi

if [ "${{1:-}}" = "issue" ] && [ "${{2:-}}" = "edit" ]; then
  printf '%s\n' "$*" >> "$LOG"

  HAS_ADD_LABEL=0
  HAS_ADD_ASSIGNEE=0
  for arg in "$@"; do
    if [ "$arg" = "--add-label" ]; then HAS_ADD_LABEL=1; fi
    if [ "$arg" = "--add-assignee" ]; then HAS_ADD_ASSIGNEE=1; fi
  done

  if [ "$HAS_ADD_LABEL" -ne 1 ] || [ "$HAS_ADD_ASSIGNEE" -ne 1 ]; then
    echo "missing add flags" >&2
    exit 5
  fi

  exit 0
fi

echo "unknown args: $*" >&2
exit 1
"#,
        log_path = log_path.display(),
        task_id = task_id,
        issue_url = issue_url,
        auth_status_block = auth_status_block
    );

    write_fake_gh_script(tmp_dir, &script)
}

#[cfg(unix)]
pub(super) fn create_fake_gh_for_issue_publish_multi(
    tmp_dir: &TempDir,
    issue_url: &str,
    auth_ok: bool,
    fail_task_id: Option<&str>,
) -> std::path::PathBuf {
    let log_path = tmp_dir.path().join("gh.log");
    let fail_task_id = fail_task_id.unwrap_or("");
    let auth_status_block = auth_status_block(auth_ok);

    let script = format!(
        r#"#!/bin/sh
set -eu

LOG="{log_path}"
FAIL_TASK_ID="{fail_task_id}"

if [ "${{1:-}}" = "--version" ]; then
  echo "gh version 0.0.0-test"
  exit 0
fi

if [ "${{1:-}}" = "auth" ] && [ "${{2:-}}" = "status" ]; then
  {auth_status_block}
fi

if [ "${{1:-}}" = "issue" ] && [ "${{2:-}}" = "create" ]; then
  printf '%s\n' "$*" >> "$LOG"

  BODY_FILE=""
  TITLE=""
  i=1
  while [ $i -le $# ]; do
    eval arg=\${{${{i}}}}
    if [ "$arg" = "--body-file" ]; then
      i=$((i+1)); eval BODY_FILE=\${{${{i}}}}
    elif [ "$arg" = "--title" ]; then
      i=$((i+1)); eval TITLE=\${{${{i}}}}
    fi
    i=$((i+1))
  done

  if [ -z "$BODY_FILE" ] || [ ! -f "$BODY_FILE" ]; then
    echo "missing body file" >&2
    exit 2
  fi
  if [ -z "$TITLE" ]; then
    echo "missing title" >&2
    exit 3
  fi

  if [ -n "$FAIL_TASK_ID" ] && grep -q "ralph_task_id: $FAIL_TASK_ID" "$BODY_FILE"; then
    echo "simulated failure for task $FAIL_TASK_ID" >&2
    exit 7
  fi

  echo "{issue_url}"
  exit 0
fi

if [ "${{1:-}}" = "issue" ] && [ "${{2:-}}" = "edit" ]; then
  printf '%s\n' "$*" >> "$LOG"

  BODY_FILE=""
  i=1
  while [ $i -le $# ]; do
    eval arg=\${{${{i}}}}
    if [ "$arg" = "--body-file" ]; then
      i=$((i+1)); eval BODY_FILE=\${{${{i}}}}
    fi
    i=$((i+1))
  done

  if [ -z "$BODY_FILE" ] || [ ! -f "$BODY_FILE" ]; then
    echo "missing body file" >&2
    exit 2
  fi

  if [ -n "$FAIL_TASK_ID" ] && grep -q "ralph_task_id: $FAIL_TASK_ID" "$BODY_FILE"; then
    echo "simulated failure for task $FAIL_TASK_ID" >&2
    exit 7
  fi
  exit 0
fi

echo "unknown args: $*" >&2
exit 1
"#,
        log_path = log_path.display(),
        issue_url = issue_url,
        fail_task_id = fail_task_id,
        auth_status_block = auth_status_block
    );

    write_fake_gh_script(tmp_dir, &script)
}
