/**
 TaskDetailSections

 Responsibilities:
 - Provide decomposed section views for `TaskDetailView`.
 - Keep form layout for task basics, content arrays, relationships, and metadata out of the root detail surface.
 - Reuse shared section chrome so task detail layout remains consistent across edits.

 Does not handle:
 - Saving or conflict detection.
 - Root-level alert and action bar orchestration.

 Invariants/assumptions callers must respect:
 - Sections expect bindings into a live `TaskDetailEditorState`.
 - Relationship controls expect the current workspace task list so cycle detection can reflect reality.
 */

import RalphCore
import SwiftUI

struct TaskDetailFormSections: View {
  @Binding var draftTask: RalphTask
  let workspace: Workspace
  let taskID: String
  let mutateTaskAgent: ((inout RalphTaskAgent) -> Void) -> Void

  var body: some View {
    VStack(alignment: .leading, spacing: 20) {
      TaskDetailBasicInfoSection(draftTask: $draftTask)
      TaskDetailStatusSection(draftTask: $draftTask)
      TaskDetailTimeTrackingSection(draftTask: $draftTask)
      TaskDetailScheduleSection(draftTask: $draftTask)
      TaskExecutionOverridesSection(
        draftTask: $draftTask,
        workspace: workspace,
        mutateTaskAgent: mutateTaskAgent
      )
      TaskDetailTagsSection(tags: $draftTask.tags)
      TaskDetailRequestSection(draftTask: $draftTask)
      TaskDetailContentSections(draftTask: $draftTask)
      TaskDetailRelationshipsSection(
        draftTask: $draftTask,
        currentTaskID: taskID,
        workspaceTasks: workspace.taskState.tasks
      )
      TaskDetailCustomFieldsSection(draftTask: $draftTask)
      TaskDetailMetadataSection(task: draftTask)
    }
  }
}

private struct TaskDetailBasicInfoSection: View {
  private enum AccessibilityID {
    static let titleField = "task-detail-title-field"
  }

  @Binding var draftTask: RalphTask

  var body: some View {
    TaskDetailSectionCard("Basic Information") {
      VStack(alignment: .leading, spacing: 16) {
        VStack(alignment: .leading, spacing: 4) {
          Text("Title")
            .font(.caption)
            .foregroundStyle(.secondary)
          TextField("Task title", text: $draftTask.title)
            .textFieldStyle(.roundedBorder)
            .accessibilityLabel("Task title")
            .accessibilityHint("Enter the task title")
            .accessibilityIdentifier(AccessibilityID.titleField)
        }

        VStack(alignment: .leading, spacing: 4) {
          Text("Description")
            .font(.caption)
            .foregroundStyle(.secondary)
          TextEditor(
            text: Binding(
              get: { draftTask.description ?? "" },
              set: { draftTask.description = $0.isEmpty ? nil : $0 }
            )
          )
          .font(.body)
          .frame(minHeight: 80, maxHeight: 120)
          .padding(4)
          .background(Color(NSColor.textBackgroundColor))
          .clipShape(.rect(cornerRadius: 6))
          .accessibilityLabel("Task description")
          .accessibilityHint("Enter a detailed description of the task")
        }
      }
    }
  }
}

private struct TaskDetailStatusSection: View {
  @Binding var draftTask: RalphTask

  var body: some View {
    TaskDetailSectionCard("Status & Priority") {
      HStack(spacing: 20) {
        VStack(alignment: .leading, spacing: 4) {
          Text("Status")
            .font(.caption)
            .foregroundStyle(.secondary)
          Picker("Status", selection: $draftTask.status) {
            ForEach(RalphTaskStatus.allCases, id: \.self) { status in
              HStack(spacing: 6) {
                Circle()
                  .fill(TaskDetailPresentation.statusColor(status))
                  .frame(width: 8, height: 8)
                  .accessibilityLabel("Status: \(status.displayName)")
                Text(status.displayName)
              }
              .tag(status)
            }
          }
          .pickerStyle(.menu)
          .frame(width: 140)
          .accessibilityLabel("Task status")
        }

        VStack(alignment: .leading, spacing: 4) {
          Text("Priority")
            .font(.caption)
            .foregroundStyle(.secondary)
          Picker("Priority", selection: $draftTask.priority) {
            ForEach(RalphTaskPriority.allCases, id: \.self) { priority in
              HStack(spacing: 6) {
                Circle()
                  .fill(TaskDetailPresentation.priorityColor(priority))
                  .frame(width: 8, height: 8)
                  .accessibilityLabel("Priority: \(priority.displayName)")
                Text(priority.displayName)
              }
              .tag(priority)
            }
          }
          .pickerStyle(.menu)
          .frame(width: 140)
          .accessibilityLabel("Task priority")
        }

        Spacer()
      }
    }
  }
}

private struct TaskDetailTimeTrackingSection: View {
  @Binding var draftTask: RalphTask

  var body: some View {
    TaskDetailSectionCard("Time Tracking") {
      HStack(spacing: 20) {
        VStack(alignment: .leading, spacing: 4) {
          Text("Estimated (min)")
            .font(.caption)
            .foregroundStyle(.secondary)
          TextField("Minutes", value: $draftTask.estimatedMinutes, format: .number)
            .textFieldStyle(.roundedBorder)
            .frame(width: 100)
            .accessibilityLabel("Estimated minutes")
        }

        VStack(alignment: .leading, spacing: 4) {
          Text("Actual (min)")
            .font(.caption)
            .foregroundStyle(.secondary)
          TextField("Minutes", value: $draftTask.actualMinutes, format: .number)
            .textFieldStyle(.roundedBorder)
            .frame(width: 100)
            .accessibilityLabel("Actual minutes")
        }

        if let estimated = draftTask.estimatedMinutes,
          let actual = draftTask.actualMinutes,
          estimated > 0
        {
          let ratio = Double(actual) / Double(estimated)
          VStack(alignment: .leading, spacing: 4) {
            Text("Accuracy")
              .font(.caption)
              .foregroundStyle(.secondary)
            HStack(spacing: 4) {
              Circle()
                .fill(TaskDetailAccuracyStyle.color(for: ratio))
                .frame(width: 8, height: 8)
              Text(TaskDetailAccuracyStyle.label(for: ratio))
                .font(.caption)
            }
          }
        }

        Spacer()
      }
    }
  }
}

private enum TaskDetailAccuracyStyle {
  static func color(for ratio: Double) -> Color {
    if ratio >= 0.75 && ratio <= 1.25 { return .green }
    if ratio >= 0.5 && ratio <= 1.5 { return .yellow }
    return .red
  }

  static func label(for ratio: Double) -> String {
    if ratio >= 0.75 && ratio <= 1.25 { return "On target" }
    if ratio >= 0.5 && ratio < 0.75 { return "Overestimated" }
    if ratio > 1.25 && ratio <= 1.5 { return "Underestimated" }
    if ratio < 0.5 { return "Way over" }
    return "Way under"
  }
}

private struct TaskDetailScheduleSection: View {
  @Binding var draftTask: RalphTask

  var body: some View {
    TaskDetailSectionCard("Schedule") {
      VStack(alignment: .leading, spacing: 12) {
        Toggle(
          "Scheduled start",
          isOn: Binding(
            get: { draftTask.scheduledStart != nil },
            set: { enabled in
              draftTask.scheduledStart = enabled ? (draftTask.scheduledStart ?? Date()) : nil
            }
          )
        )
        .accessibilityLabel("Scheduled start enabled")

        if draftTask.scheduledStart != nil {
          DatePicker(
            "Start",
            selection: Binding(
              get: { draftTask.scheduledStart ?? Date() },
              set: { draftTask.scheduledStart = $0 }
            ),
            displayedComponents: [.date, .hourAndMinute]
          )
          .accessibilityLabel("Scheduled start date and time")
        }
      }
    }
  }
}

private struct TaskDetailTagsSection: View {
  @Binding var tags: [String]

  var body: some View {
    TaskDetailSectionCard("Tags") {
      TagEditorView(tags: $tags)
    }
  }
}

private struct TaskDetailRequestSection: View {
  @Binding var draftTask: RalphTask

  var body: some View {
    TaskDetailSectionCard("Original Request") {
      TextEditor(
        text: Binding(
          get: { draftTask.request ?? "" },
          set: {
            draftTask.request =
              $0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : $0
          }
        )
      )
      .font(.body)
      .frame(minHeight: 72, maxHeight: 120)
      .padding(4)
      .background(Color(NSColor.textBackgroundColor))
      .clipShape(.rect(cornerRadius: 6))
      .accessibilityLabel("Original request")
    }
  }
}

private struct TaskDetailContentSections: View {
  @Binding var draftTask: RalphTask

  var body: some View {
    Group {
      if draftTask.scope != nil {
        TaskDetailStringArraySection(
          title: "Scope",
          items: Binding(
            get: { draftTask.scope ?? [] },
            set: { draftTask.scope = $0.isEmpty ? nil : $0 }
          ),
          placeholder: "Add file path..."
        )
      }

      if draftTask.evidence != nil {
        TaskDetailStringArraySection(
          title: "Evidence",
          items: Binding(
            get: { draftTask.evidence ?? [] },
            set: { draftTask.evidence = $0.isEmpty ? nil : $0 }
          ),
          placeholder: "Add evidence item..."
        )
      }

      if draftTask.plan != nil {
        TaskDetailStringArraySection(
          title: "Plan",
          items: Binding(
            get: { draftTask.plan ?? [] },
            set: { draftTask.plan = $0.isEmpty ? nil : $0 }
          ),
          placeholder: "Add plan step..."
        )
      }

      if draftTask.notes != nil {
        TaskDetailStringArraySection(
          title: "Notes",
          items: Binding(
            get: { draftTask.notes ?? [] },
            set: { draftTask.notes = $0.isEmpty ? nil : $0 }
          ),
          placeholder: "Add note..."
        )
      }

      TaskDetailSectionCard("Add Fields") {
        FlowLayout(spacing: 8) {
          if draftTask.scope == nil {
            TaskDetailAddFieldButton(title: "+ Scope") { draftTask.scope = [] }
          }
          if draftTask.evidence == nil {
            TaskDetailAddFieldButton(title: "+ Evidence") { draftTask.evidence = [] }
          }
          if draftTask.plan == nil {
            TaskDetailAddFieldButton(title: "+ Plan") { draftTask.plan = [] }
          }
          if draftTask.notes == nil {
            TaskDetailAddFieldButton(title: "+ Notes") { draftTask.notes = [] }
          }
        }
      }
    }
  }
}

private struct TaskDetailStringArraySection: View {
  let title: String
  let items: Binding<[String]>
  let placeholder: String

  var body: some View {
    TaskDetailSectionCard(title) {
      StringArrayEditor(items: items, placeholder: placeholder)
    }
  }
}

private struct TaskDetailRelationshipsSection: View {
  @Binding var draftTask: RalphTask
  let currentTaskID: String
  let workspaceTasks: [RalphTask]

  var body: some View {
    let allTaskIDs = workspaceTasks.map(\.id).filter { $0 != currentTaskID }
    let existingEdges = TaskDetailPresentation.existingEdges(from: workspaceTasks)

    return TaskDetailSectionCard("Relationships") {
      VStack(alignment: .leading, spacing: 16) {
        if draftTask.dependsOn != nil {
          TaskRelationshipPicker(
            label: "Depends On",
            relatedTaskIDs: Binding(
              get: { draftTask.dependsOn ?? [] },
              set: { draftTask.dependsOn = $0.isEmpty ? nil : $0 }
            ),
            allTaskIDs: allTaskIDs,
            currentTaskID: currentTaskID,
            edgeType: .dependency,
            existingEdges: existingEdges
          )
        }

        if draftTask.blocks != nil {
          TaskRelationshipPicker(
            label: "Blocks",
            relatedTaskIDs: Binding(
              get: { draftTask.blocks ?? [] },
              set: { draftTask.blocks = $0.isEmpty ? nil : $0 }
            ),
            allTaskIDs: allTaskIDs,
            currentTaskID: currentTaskID,
            edgeType: .blocks,
            existingEdges: existingEdges
          )
        }

        if draftTask.relatesTo != nil {
          TaskRelationshipPicker(
            label: "Relates To",
            relatedTaskIDs: Binding(
              get: { draftTask.relatesTo ?? [] },
              set: { draftTask.relatesTo = $0.isEmpty ? nil : $0 }
            ),
            allTaskIDs: allTaskIDs,
            currentTaskID: currentTaskID,
            edgeType: .relatesTo,
            existingEdges: existingEdges
          )
        }

        if draftTask.dependsOn == nil || draftTask.blocks == nil || draftTask.relatesTo == nil {
          FlowLayout(spacing: 8) {
            if draftTask.dependsOn == nil {
              TaskDetailAddFieldButton(title: "+ Depends On") { draftTask.dependsOn = [] }
            }
            if draftTask.blocks == nil {
              TaskDetailAddFieldButton(title: "+ Blocks") { draftTask.blocks = [] }
            }
            if draftTask.relatesTo == nil {
              TaskDetailAddFieldButton(title: "+ Relates To") { draftTask.relatesTo = [] }
            }
          }
        }

        VStack(alignment: .leading, spacing: 4) {
          Text("Duplicates")
            .font(.caption)
            .foregroundStyle(.secondary)
          TextField(
            "Duplicate task ID",
            text: Binding(
              get: { draftTask.duplicates ?? "" },
              set: {
                draftTask.duplicates =
                  $0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : $0
              }
            )
          )
          .textFieldStyle(.roundedBorder)
          .accessibilityLabel("Duplicate task ID")
        }
      }
    }
  }
}

private struct TaskDetailCustomFieldsSection: View {
  @Binding var draftTask: RalphTask

  var body: some View {
    TaskDetailSectionCard("Custom Fields") {
      TextEditor(
        text: Binding(
          get: { Self.render(draftTask.customFields ?? [:]) },
          set: { draftTask.customFields = Self.parse($0) }
        )
      )
      .font(.body.monospaced())
      .frame(minHeight: 72, maxHeight: 120)
      .padding(4)
      .background(Color(NSColor.textBackgroundColor))
      .clipShape(.rect(cornerRadius: 6))
      .accessibilityLabel("Custom fields")
      .accessibilityHint("Enter one key equals value pair per line")
    }
  }

  private static func render(_ fields: [String: String]) -> String {
    fields
      .sorted { $0.key < $1.key }
      .map { "\($0.key)=\($0.value)" }
      .joined(separator: "\n")
  }

  private static func parse(_ value: String) -> [String: String]? {
    var fields: [String: String] = [:]
    for line in value.split(whereSeparator: \.isNewline) {
      let parts = line.split(separator: "=", maxSplits: 1).map(String.init)
      guard parts.count == 2 else { continue }
      let key = parts[0].trimmingCharacters(in: .whitespacesAndNewlines)
      let fieldValue = parts[1].trimmingCharacters(in: .whitespacesAndNewlines)
      if !key.isEmpty {
        fields[key] = fieldValue
      }
    }
    return fields.isEmpty ? nil : fields
  }
}

private struct TaskDetailMetadataSection: View {
  let task: RalphTask

  var body: some View {
    TaskDetailSectionCard("Metadata") {
      VStack(alignment: .leading, spacing: 8) {
        TaskDetailMetadataRow(label: "Created", date: task.createdAt)
        TaskDetailMetadataRow(label: "Updated", date: task.updatedAt)
        TaskDetailMetadataRow(label: "Started", date: task.startedAt)
        TaskDetailMetadataRow(label: "Completed", date: task.completedAt)
      }
    }
  }
}

private struct TaskDetailMetadataRow: View {
  let label: String
  let date: Date?

  var body: some View {
    HStack {
      Text(label)
        .font(.caption)
        .foregroundStyle(.secondary)
        .frame(width: 70, alignment: .leading)

      if let date {
        Text(TaskDetailPresentation.formatDate(date))
          .font(.caption)
          .foregroundStyle(.primary)
      } else {
        Text("—")
          .font(.caption)
          .foregroundStyle(.secondary)
      }

      Spacer()
    }
    .accessibilityLabel(
      "\(label): \(date.map(TaskDetailPresentation.formatDateForAccessibility) ?? "Not set")")
  }
}

private struct TaskDetailAddFieldButton: View {
  let title: String
  let action: () -> Void

  var body: some View {
    Button(action: action) {
      Text(title)
        .font(.caption)
        .padding(.horizontal, 10)
        .padding(.vertical, 4)
    }
    .buttonStyle(GlassButtonStyle())
    .accessibilityLabel("Add \(title) field")
  }
}

private struct TaskDetailSectionCard<Content: View>: View {
  let title: String
  @ViewBuilder let content: () -> Content

  init(_ title: String, @ViewBuilder content: @escaping () -> Content) {
    self.title = title
    self.content = content
  }

  var body: some View {
    VStack(alignment: .leading, spacing: 8) {
      Text(title)
        .font(.system(.caption, weight: .semibold))
        .foregroundStyle(.secondary)
        .padding(.horizontal, 12)

      content()
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .underPageBackground(cornerRadius: 10, isEmphasized: false)
    }
    .accessibilityLabel("\(title) section")
  }
}
