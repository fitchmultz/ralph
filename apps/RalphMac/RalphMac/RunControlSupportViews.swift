//!
//! RunControlSupportViews
//!
//! Purpose:
//! - Hold reusable Run Control view helpers and micro-components.
//!
//! Responsibilities:
//! - Provide consistent section chrome, tag chips, config rows, history rows, and duration formatting.
//!
//! Scope:
//! - Shared Run Control visuals only.
//!
//! Usage:
//! - Used by the decomposed Run Control section files.
//!
//! Invariants/Assumptions:
//! - These helpers stay presentation-focused and do not own workspace orchestration.

import RalphCore
import SwiftUI

enum RunControlDurationFormatter {
    static func string(for duration: TimeInterval) -> String {
        if duration < 60 {
            return String(format: "%.0fs", duration)
        }
        let minutes = Int(duration) / 60
        let seconds = Int(duration) % 60
        return String(format: "%d:%02d", minutes, seconds)
    }
}

@MainActor
struct RunControlGlassSection<Content: View>: View {
    let title: String
    @ViewBuilder let content: Content

    init(_ title: String, @ViewBuilder content: () -> Content) {
        self.title = title
        self.content = content()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title)
                .font(.system(.caption, weight: .semibold))
                .foregroundStyle(.secondary)
                .padding(.horizontal, 12)

            content
                .padding(12)
                .frame(maxWidth: .infinity, alignment: .leading)
                .underPageBackground(cornerRadius: 10, isEmphasized: false)
        }
    }
}

@MainActor
struct RunControlTagChips: View {
    let tags: [String]

    var body: some View {
        HStack(spacing: 4) {
            ForEach(tags, id: \.self) { tag in
                Text(tag)
                    .font(.caption2)
                    .padding(.horizontal, 6)
                    .padding(.vertical, 2)
                    .background(.quaternary.opacity(0.3))
                    .clipShape(RoundedRectangle(cornerRadius: 4))
            }
        }
    }
}

@MainActor
struct RunControlConfigRow: View {
    let icon: String
    let label: String
    let value: String

    var body: some View {
        HStack {
            Image(systemName: icon)
                .foregroundStyle(.secondary)
                .frame(width: 20)
            Text(label)
                .foregroundStyle(.secondary)
            Spacer()
            Text(value)
                .font(.system(.body, design: .monospaced))
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(label): \(value)")
    }
}

@MainActor
struct RunControlExecutionHistoryRow: View {
    let record: Workspace.ExecutionRecord

    var body: some View {
        HStack {
            RunControlExecutionStatusIcon(record: record)

            if let taskID = record.taskID {
                Text(taskID)
                    .font(.system(.caption, design: .monospaced))
            } else {
                Text("Unknown task")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Spacer()

            if let duration = record.duration {
                Text(RunControlDurationFormatter.string(for: duration))
                    .font(.system(.caption, design: .monospaced))
                    .foregroundStyle(.secondary)
            }
        }
    }
}

@MainActor
struct RunControlExecutionStatusIcon: View {
    let record: Workspace.ExecutionRecord

    var body: some View {
        Image(systemName: iconName)
            .foregroundStyle(iconColor)
    }

    private var iconName: String {
        if record.wasCancelled {
            return "xmark.octagon.fill"
        }
        return record.success ? "checkmark.circle.fill" : "xmark.circle.fill"
    }

    private var iconColor: Color {
        if record.wasCancelled {
            return .orange
        }
        return record.success ? .green : .red
    }
}
