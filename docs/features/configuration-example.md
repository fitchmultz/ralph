# Complete Configuration Example
Status: Active
Owner: Maintainers
Source of truth: this document for the complete feature-configuration example
Parent: [Configuration Feature Guide](configuration.md)

## Complete Configuration Example

Here's a comprehensive example demonstrating all configuration sections:

```jsonc
{
  // Schema version (required)
  "version": 2,
  
  // Project type affects prompt defaults
  "project_type": "code",
  
  // Agent execution settings
  "agent": {
    "runner": "claude",
    "model": "sonnet",
    "phases": 3,
    "iterations": 1,
    "reasoning_effort": "high",
    
    // Runner binaries
    "claude_bin": "claude",
    "codex_bin": "codex",
    
    // Safety settings
    "claude_permission_mode": "bypass_permissions",
    "git_revert_mode": "ask",
    "git_publish_mode": "commit_and_push",
    
    // CI gate
    "ci_gate": {
      "enabled": true,
      "argv": ["make", "ci"]
    },
    
    // RepoPrompt integration
    "repoprompt_plan_required": false,
    "instruction_files": ["AGENTS.md"],
    
    // Phase-specific overrides
    "phase_overrides": {
      "phase1": {
        "model": "o3-mini",
        "reasoning_effort": "high"
      }
    },
    
    // Runner CLI normalization
    "runner_cli": {
      "defaults": {
        "approval_mode": "yolo",
        "output_format": "stream_json"
      },
      "runners": {
        "codex": { "sandbox": "disabled" }
      }
    },
    
    // Retry configuration
    "runner_retry": {
      "max_attempts": 3,
      "base_backoff_ms": 1000
    },
    
    // Notifications
    "notification": {
      "notify_on_complete": true,
      "notify_on_fail": true,
      "sound_enabled": false
    },
    
    // Webhooks
    "webhook": {
      "enabled": true,
      "url": "${WEBHOOK_URL}",
      "secret": "${WEBHOOK_SECRET}",
      "events": ["task_completed", "task_failed"]
    },
    
    // Session management
    "session_timeout_hours": 24,
    "scan_prompt_version": "v2"
  },
  
  // Parallel execution
  "parallel": {
    "workers": 3,
    "workspace_root": ".workspaces/my-repo/parallel",
    "max_push_attempts": 50,
    "push_backoff_ms": [500, 2000, 5000, 10000],
    "workspace_retention_hours": 24
  },
  
  // Queue configuration
  "queue": {
    "file": ".ralph/queue.jsonc",
    "done_file": ".ralph/done.jsonc",
    "id_prefix": "RQ",
    "id_width": 4,
    "auto_archive_terminal_after_days": 7,
    "aging_thresholds": {
      "warning_days": 7,
      "stale_days": 14,
      "rotten_days": 30
    }
  },
  
  // Plugin configuration
  "plugins": {
    "plugins": {
      "custom.runner": {
        "enabled": true,
        "runner": { "bin": "custom-runner" }
      }
    }
  },
  
  // Custom profiles
  "profiles": {
    "fast-local": {
      "runner": "kimi",
      "phases": 1
    },
    "deep-review": {
      "runner": "claude",
      "model": "opus",
      "phases": 3
    }
  }
}
```

---


