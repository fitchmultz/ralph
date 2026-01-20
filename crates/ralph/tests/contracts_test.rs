// contracts_test.rs - Unit tests for contracts.rs (Task validation, serialization/deserialization)

use ralph::contracts::{
    AgentConfig, ClaudePermissionMode, Config, Model, ProjectType, QueueConfig, QueueFile,
    ReasoningEffort, Runner, Task, TaskAgent, TaskPriority, TaskStatus,
};
use serde_json::Value;
use std::path::PathBuf;

#[test]
fn test_task_default_status() {
    let task = Task {
        id: "RQ-0001".to_string(),
        title: "Test task".to_string(),
        ..Default::default()
    };
    assert_eq!(task.status, TaskStatus::Todo);
    assert_eq!(task.priority, TaskPriority::Medium);
    assert!(task.tags.is_empty());
    assert!(task.scope.is_empty());
    assert!(task.evidence.is_empty());
    assert!(task.plan.is_empty());
    assert!(task.notes.is_empty());
    assert!(task.depends_on.is_empty());
}

#[test]
fn test_task_status_display() {
    assert_eq!(TaskStatus::Todo.to_string(), "todo");
    assert_eq!(TaskStatus::Doing.to_string(), "doing");
    assert_eq!(TaskStatus::Done.to_string(), "done");
    assert_eq!(TaskStatus::Rejected.to_string(), "rejected");
}

#[test]
fn test_task_status_as_str() {
    assert_eq!(TaskStatus::Todo.as_str(), "todo");
    assert_eq!(TaskStatus::Doing.as_str(), "doing");
    assert_eq!(TaskStatus::Done.as_str(), "done");
    assert_eq!(TaskStatus::Rejected.as_str(), "rejected");
}

#[test]
fn test_task_priority_display() {
    assert_eq!(TaskPriority::Critical.to_string(), "critical");
    assert_eq!(TaskPriority::High.to_string(), "high");
    assert_eq!(TaskPriority::Medium.to_string(), "medium");
    assert_eq!(TaskPriority::Low.to_string(), "low");
}

#[test]
fn test_task_priority_as_str() {
    assert_eq!(TaskPriority::Critical.as_str(), "critical");
    assert_eq!(TaskPriority::High.as_str(), "high");
    assert_eq!(TaskPriority::Medium.as_str(), "medium");
    assert_eq!(TaskPriority::Low.as_str(), "low");
}

#[test]
fn test_task_priority_weight() {
    assert_eq!(TaskPriority::Critical.weight(), 3);
    assert_eq!(TaskPriority::High.weight(), 2);
    assert_eq!(TaskPriority::Medium.weight(), 1);
    assert_eq!(TaskPriority::Low.weight(), 0);
}

#[test]
fn test_task_priority_ordering() {
    assert!(TaskPriority::Critical > TaskPriority::High);
    assert!(TaskPriority::High > TaskPriority::Medium);
    assert!(TaskPriority::Medium > TaskPriority::Low);
    assert!(TaskPriority::Critical > TaskPriority::Low);
}

#[test]
fn test_task_priority_equality() {
    assert_eq!(TaskPriority::Critical, TaskPriority::Critical);
    assert_eq!(TaskPriority::High, TaskPriority::High);
    assert_eq!(TaskPriority::Medium, TaskPriority::Medium);
    assert_eq!(TaskPriority::Low, TaskPriority::Low);
}

#[test]
fn test_task_status_equality() {
    assert_eq!(TaskStatus::Todo, TaskStatus::Todo);
    assert_eq!(TaskStatus::Doing, TaskStatus::Doing);
    assert_eq!(TaskStatus::Done, TaskStatus::Done);
    assert_eq!(TaskStatus::Rejected, TaskStatus::Rejected);
}

#[test]
fn test_task_serialization_minimal() {
    let task = Task {
        id: "RQ-0001".to_string(),
        title: "Test task".to_string(),
        ..Default::default()
    };

    let json = serde_json::to_string(&task).unwrap();
    let value: Value = serde_json::from_str(&json).unwrap();

    assert_eq!(value["id"], "RQ-0001");
    assert_eq!(value["title"], "Test task");
    assert_eq!(value["status"], "todo");
    assert_eq!(value["priority"], "medium");
    assert!(value["tags"].is_array());
    assert_eq!(value["tags"].as_array().unwrap().len(), 0);
}

#[test]
fn test_task_serialization_full() {
    let task = Task {
        id: "RQ-0001".to_string(),
        status: TaskStatus::Doing,
        title: "Test task".to_string(),
        priority: TaskPriority::High,
        tags: vec!["rust".to_string(), "testing".to_string()],
        scope: vec!["crates/ralph/src/contracts.rs".to_string()],
        evidence: vec!["Evidence 1".to_string()],
        plan: vec!["Plan step 1".to_string()],
        notes: vec!["Note 1".to_string()],
        request: Some("Test request".to_string()),
        agent: Some(TaskAgent {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt52Codex),
            reasoning_effort: Some(ReasoningEffort::Medium),
        }),
        created_at: Some("2025-01-19T00:00:00Z".to_string()),
        updated_at: Some("2025-01-19T01:00:00Z".to_string()),
        completed_at: None,
        depends_on: vec!["RQ-0000".to_string()],
        custom_fields: std::collections::HashMap::new(),
    };

    let json = serde_json::to_string(&task).unwrap();
    let value: Value = serde_json::from_str(&json).unwrap();

    assert_eq!(value["id"], "RQ-0001");
    assert_eq!(value["status"], "doing");
    assert_eq!(value["priority"], "high");
    assert_eq!(value["tags"].as_array().unwrap().len(), 2);
    assert_eq!(value["scope"].as_array().unwrap().len(), 1);
    assert_eq!(value["evidence"].as_array().unwrap().len(), 1);
    assert_eq!(value["plan"].as_array().unwrap().len(), 1);
    assert_eq!(value["notes"].as_array().unwrap().len(), 1);
    assert_eq!(value["request"], "Test request");
    assert_eq!(value["agent"]["runner"], "codex");
    assert_eq!(value["agent"]["model"], "gpt-5.2-codex");
    assert_eq!(value["agent"]["reasoning_effort"], "medium");
    assert_eq!(value["created_at"], "2025-01-19T00:00:00Z");
    assert!(value["completed_at"].is_null());
    assert_eq!(value["depends_on"].as_array().unwrap().len(), 1);
}

#[test]
fn test_task_serialization_done_with_completed_at() {
    let task = Task {
        id: "RQ-0001".to_string(),
        status: TaskStatus::Done,
        title: "Completed task".to_string(),
        completed_at: Some("2025-01-19T02:00:00Z".to_string()),
        ..Default::default()
    };

    let json = serde_json::to_string(&task).unwrap();
    let value: Value = serde_json::from_str(&json).unwrap();

    assert_eq!(value["status"], "done");
    assert_eq!(value["completed_at"], "2025-01-19T02:00:00Z");
}

#[test]
fn test_task_deserialization_minimal() {
    let json = r#"{"id":"RQ-0001","title":"Test task"}"#;
    let task: Task = serde_json::from_str(json).unwrap();

    assert_eq!(task.id, "RQ-0001");
    assert_eq!(task.title, "Test task");
    assert_eq!(task.status, TaskStatus::Todo);
    assert_eq!(task.priority, TaskPriority::Medium);
    assert!(task.tags.is_empty());
}

#[test]
fn test_task_deserialization_full() {
    let json = r#"{
        "id":"RQ-0001",
        "status":"doing",
        "title":"Test task",
        "priority":"high",
        "tags":["rust","testing"],
        "scope":["crates/ralph/src/contracts.rs"],
        "evidence":["Evidence 1"],
        "plan":["Plan step 1"],
        "notes":["Note 1"],
        "request":"Test request",
        "agent":{
            "runner":"codex",
            "model":"gpt-5.2-codex",
            "reasoning_effort":"medium"
        },
        "created_at":"2025-01-19T00:00:00Z",
        "updated_at":"2025-01-19T01:00:00Z",
        "depends_on":["RQ-0000"]
    }"#;
    let task: Task = serde_json::from_str(json).unwrap();

    assert_eq!(task.id, "RQ-0001");
    assert_eq!(task.status, TaskStatus::Doing);
    assert_eq!(task.title, "Test task");
    assert_eq!(task.priority, TaskPriority::High);
    assert_eq!(task.tags.len(), 2);
    assert_eq!(task.tags[0], "rust");
    assert_eq!(task.agent.as_ref().unwrap().runner, Some(Runner::Codex));
    assert_eq!(task.created_at, Some("2025-01-19T00:00:00Z".to_string()));
}

#[test]
fn test_task_deserialization_rejects_unknown_fields() {
    let json = r#"{"id":"RQ-0001","title":"Test task","unknown_field":"value"}"#;
    let result: Result<Task, _> = serde_json::from_str(json);
    assert!(result.is_err(), "Should reject unknown fields");
}

#[test]
fn test_task_serialization_roundtrip() {
    let original = Task {
        id: "RQ-0001".to_string(),
        status: TaskStatus::Doing,
        title: "Test task".to_string(),
        priority: TaskPriority::High,
        tags: vec!["rust".to_string()],
        scope: vec!["crates/ralph/src/contracts.rs".to_string()],
        evidence: vec!["Evidence".to_string()],
        plan: vec!["Plan".to_string()],
        notes: vec!["Note".to_string()],
        request: Some("Request".to_string()),
        agent: Some(TaskAgent {
            runner: Some(Runner::Codex),
            model: Some(Model::Gpt52Codex),
            reasoning_effort: Some(ReasoningEffort::Medium),
        }),
        created_at: Some("2025-01-19T00:00:00Z".to_string()),
        updated_at: Some("2025-01-19T01:00:00Z".to_string()),
        completed_at: None,
        depends_on: vec![],
        custom_fields: std::collections::HashMap::new(),
    };

    let json = serde_json::to_string(&original).unwrap();
    let deserialized: Task = serde_json::from_str(&json).unwrap();

    assert_eq!(deserialized.id, original.id);
    assert_eq!(deserialized.status, original.status);
    assert_eq!(deserialized.title, original.title);
    assert_eq!(deserialized.priority, original.priority);
    assert_eq!(deserialized.tags, original.tags);
    assert_eq!(deserialized.scope, original.scope);
    assert_eq!(deserialized.evidence, original.evidence);
    assert_eq!(deserialized.plan, original.plan);
    assert_eq!(deserialized.notes, original.notes);
    assert_eq!(deserialized.request, original.request);
    assert_eq!(deserialized.created_at, original.created_at);
    assert_eq!(deserialized.updated_at, original.updated_at);
    assert_eq!(deserialized.completed_at, original.completed_at);
    assert_eq!(deserialized.depends_on, original.depends_on);
}

#[test]
fn test_model_default() {
    assert_eq!(Model::default(), Model::Gpt52Codex);
}

#[test]
fn test_model_as_str() {
    assert_eq!(Model::Gpt52Codex.as_str(), "gpt-5.2-codex");
    assert_eq!(Model::Gpt52.as_str(), "gpt-5.2");
    assert_eq!(Model::Glm47.as_str(), "zai-coding-plan/glm-4.7");
    assert_eq!(
        Model::Custom("custom-model".to_string()).as_str(),
        "custom-model"
    );
}

#[test]
fn test_model_from_str() {
    assert_eq!("gpt-5.2-codex".parse::<Model>(), Ok(Model::Gpt52Codex));
    assert_eq!("gpt-5.2".parse::<Model>(), Ok(Model::Gpt52));
    assert_eq!("zai-coding-plan/glm-4.7".parse::<Model>(), Ok(Model::Glm47));
    assert_eq!(
        "custom-model".parse::<Model>(),
        Ok(Model::Custom("custom-model".to_string()))
    );
}

#[test]
fn test_model_from_str_whitespace() {
    assert_eq!("  gpt-5.2-codex  ".parse::<Model>(), Ok(Model::Gpt52Codex));
}

#[test]
fn test_model_from_str_empty() {
    assert!("".parse::<Model>().is_err());
    assert!("  ".parse::<Model>().is_err());
}

#[test]
fn test_model_serialization() {
    let model = Model::Gpt52Codex;
    let json = serde_json::to_string(&model).unwrap();
    assert_eq!(json, "\"gpt-5.2-codex\"");

    let custom = Model::Custom("my-model".to_string());
    let json = serde_json::to_string(&custom).unwrap();
    assert_eq!(json, "\"my-model\"");
}

#[test]
fn test_model_deserialization() {
    let json = "\"gpt-5.2-codex\"";
    let model: Model = serde_json::from_str(json).unwrap();
    assert_eq!(model, Model::Gpt52Codex);

    let json = "\"custom-model\"";
    let model: Model = serde_json::from_str(json).unwrap();
    assert_eq!(model, Model::Custom("custom-model".to_string()));
}

#[test]
fn test_queue_file_default() {
    let queue = QueueFile::default();
    assert_eq!(queue.version, 1);
    assert!(queue.tasks.is_empty());
}

#[test]
fn test_queue_file_serialization() {
    let queue = QueueFile {
        version: 1,
        tasks: vec![Task {
            id: "RQ-0001".to_string(),
            title: "Test".to_string(),
            ..Default::default()
        }],
    };

    let json = serde_json::to_string(&queue).unwrap();
    let value: Value = serde_json::from_str(&json).unwrap();

    assert_eq!(value["version"], 1);
    assert_eq!(value["tasks"].as_array().unwrap().len(), 1);
    assert_eq!(value["tasks"][0]["id"], "RQ-0001");
}

#[test]
fn test_queue_file_deserialization() {
    let json = r#"{"version":1,"tasks":[{"id":"RQ-0001","title":"Test"}]}"#;
    let queue: QueueFile = serde_json::from_str(json).unwrap();

    assert_eq!(queue.version, 1);
    assert_eq!(queue.tasks.len(), 1);
    assert_eq!(queue.tasks[0].id, "RQ-0001");
}

#[test]
fn test_queue_config_default() {
    let config = QueueConfig::default();
    assert!(config.file.is_none());
    assert!(config.done_file.is_none());
    assert!(config.id_prefix.is_none());
    assert!(config.id_width.is_none());
}

#[test]
fn test_queue_config_merge_from() {
    let mut base = QueueConfig {
        file: Some(PathBuf::from("base.json")),
        done_file: Some(PathBuf::from("base-done.json")),
        id_prefix: Some("BASE".to_string()),
        id_width: Some(4),
    };

    let override_config = QueueConfig {
        file: Some(PathBuf::from("override.json")),
        done_file: None,
        id_prefix: Some("OVR".to_string()),
        id_width: None,
    };

    base.merge_from(override_config);

    assert_eq!(base.file, Some(PathBuf::from("override.json")));
    assert_eq!(base.done_file, Some(PathBuf::from("base-done.json")));
    assert_eq!(base.id_prefix, Some("OVR".to_string()));
    assert_eq!(base.id_width, Some(4));
}

#[test]
fn test_agent_config_default() {
    let config = AgentConfig::default();
    assert!(config.runner.is_none());
    assert!(config.model.is_none());
    assert!(config.reasoning_effort.is_none());
}

#[test]
fn test_agent_config_merge_from() {
    let mut base = AgentConfig {
        runner: Some(Runner::Codex),
        model: Some(Model::Gpt52Codex),
        reasoning_effort: Some(ReasoningEffort::Medium),
        codex_bin: Some("codex".to_string()),
        opencode_bin: Some("opencode".to_string()),
        gemini_bin: Some("gemini".to_string()),
        claude_bin: Some("claude".to_string()),
        two_pass_plan: Some(true),
        claude_permission_mode: Some(ClaudePermissionMode::BypassPermissions),
    };

    let override_config = AgentConfig {
        runner: Some(Runner::Opencode),
        model: None,
        reasoning_effort: None,
        codex_bin: None,
        opencode_bin: Some("opencode-custom".to_string()),
        gemini_bin: None,
        claude_bin: None,
        two_pass_plan: Some(false),
        claude_permission_mode: None,
    };

    base.merge_from(override_config);

    assert_eq!(base.runner, Some(Runner::Opencode));
    assert_eq!(base.model, Some(Model::Gpt52Codex));
    assert_eq!(base.reasoning_effort, Some(ReasoningEffort::Medium));
    assert_eq!(base.codex_bin, Some("codex".to_string()));
    assert_eq!(base.opencode_bin, Some("opencode-custom".to_string()));
    assert_eq!(base.gemini_bin, Some("gemini".to_string()));
    assert_eq!(base.claude_bin, Some("claude".to_string()));
    assert_eq!(base.two_pass_plan, Some(false));
    assert_eq!(
        base.claude_permission_mode,
        Some(ClaudePermissionMode::BypassPermissions)
    );
}

#[test]
fn test_config_default() {
    let config = Config::default();
    assert_eq!(config.version, 1);
    assert_eq!(config.project_type, Some(ProjectType::Code));
    assert_eq!(config.queue.file, Some(PathBuf::from(".ralph/queue.json")));
    assert_eq!(
        config.queue.done_file,
        Some(PathBuf::from(".ralph/done.json"))
    );
    assert_eq!(config.queue.id_prefix, Some("RQ".to_string()));
    assert_eq!(config.queue.id_width, Some(4));
    assert_eq!(config.agent.runner, Some(Runner::Claude));
    assert_eq!(
        config.agent.model,
        Some(Model::Custom("sonnet".to_string()))
    );
}

#[test]
fn test_task_agent_defaults() {
    let agent = TaskAgent {
        runner: None,
        model: None,
        reasoning_effort: None,
    };

    let json = serde_json::to_string(&agent).unwrap();
    let value: Value = serde_json::from_str(&json).unwrap();

    // None values should not be serialized
    assert!(value.get("runner").is_none() || value["runner"].is_null());
    assert!(value.get("model").is_none() || value["model"].is_null());
}

#[test]
fn test_runner_default() {
    assert_eq!(Runner::default(), Runner::Claude);
}

#[test]
fn test_reasoning_effort_default() {
    assert_eq!(ReasoningEffort::default(), ReasoningEffort::Medium);
}

#[test]
fn test_claude_permission_mode_default() {
    assert_eq!(
        ClaudePermissionMode::default(),
        ClaudePermissionMode::AcceptEdits
    );
}

#[test]
fn test_task_with_empty_optional_fields_serialization() {
    let task = Task {
        id: "RQ-0001".to_string(),
        title: "Test".to_string(),
        request: None,
        agent: None,
        created_at: None,
        updated_at: None,
        completed_at: None,
        ..Default::default()
    };

    let json = serde_json::to_string(&task).unwrap();
    let value: Value = serde_json::from_str(&json).unwrap();

    // Optional None fields should not appear in output
    assert!(value.get("request").is_none());
    assert!(value.get("agent").is_none());
    assert!(value.get("created_at").is_none());
    assert!(value.get("updated_at").is_none());
    assert!(value.get("completed_at").is_none());
}

#[test]
fn test_task_status_hash() {
    use std::collections::HashSet;
    let mut set = HashSet::new();
    set.insert(TaskStatus::Todo);
    set.insert(TaskStatus::Todo);
    set.insert(TaskStatus::Doing);

    assert_eq!(set.len(), 2);
    assert!(set.contains(&TaskStatus::Todo));
    assert!(set.contains(&TaskStatus::Doing));
}

#[test]
fn test_task_priority_hash() {
    use std::collections::HashSet;
    let mut set = HashSet::new();
    set.insert(TaskPriority::High);
    set.insert(TaskPriority::High);
    set.insert(TaskPriority::Low);

    assert_eq!(set.len(), 2);
    assert!(set.contains(&TaskPriority::High));
    assert!(set.contains(&TaskPriority::Low));
}

#[test]
fn test_model_equality() {
    assert_eq!(Model::Gpt52Codex, Model::Gpt52Codex);
    assert_eq!(Model::Gpt52, Model::Gpt52);
    assert_eq!(Model::Glm47, Model::Glm47);
    assert_eq!(
        Model::Custom("test".to_string()),
        Model::Custom("test".to_string())
    );
    assert_ne!(
        Model::Custom("test".to_string()),
        Model::Custom("other".to_string())
    );
}

#[test]
fn test_runner_equality() {
    assert_eq!(Runner::Codex, Runner::Codex);
    assert_eq!(Runner::Opencode, Runner::Opencode);
    assert_eq!(Runner::Gemini, Runner::Gemini);
    assert_eq!(Runner::Claude, Runner::Claude);
}

#[test]
fn test_reasoning_effort_equality() {
    assert_eq!(ReasoningEffort::Minimal, ReasoningEffort::Minimal);
    assert_eq!(ReasoningEffort::Low, ReasoningEffort::Low);
    assert_eq!(ReasoningEffort::Medium, ReasoningEffort::Medium);
    assert_eq!(ReasoningEffort::High, ReasoningEffort::High);
}

#[test]
fn test_task_depends_on_empty() {
    let task = Task {
        id: "RQ-0001".to_string(),
        title: "Test".to_string(),
        ..Default::default()
    };

    assert!(task.depends_on.is_empty());

    let json = serde_json::to_string(&task).unwrap();
    let value: Value = serde_json::from_str(&json).unwrap();
    assert!(value["depends_on"].is_array());
    assert!(value["depends_on"].as_array().unwrap().is_empty());
}

#[test]
fn test_task_depends_on_multiple() {
    let task = Task {
        id: "RQ-0003".to_string(),
        title: "Test".to_string(),
        depends_on: vec!["RQ-0001".to_string(), "RQ-0002".to_string()],
        ..Default::default()
    };

    assert_eq!(task.depends_on.len(), 2);

    let json = serde_json::to_string(&task).unwrap();
    let deserialized: Task = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.depends_on.len(), 2);
    assert_eq!(deserialized.depends_on[0], "RQ-0001");
    assert_eq!(deserialized.depends_on[1], "RQ-0002");
}
