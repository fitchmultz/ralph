/**
 ConnectionStatusViews

 Purpose:
 - Display workspace operational-health indicators for CLI, watcher, and persistence state.

 Responsibilities:
 - Display workspace operational-health indicators for CLI, watcher, and persistence state.
 - Provide inline banner when the workspace has degraded or failed runtime health.
 - Provide smaller inline indicator for sidebars and toolbars plus a detailed health sheet.

 Does not handle:
 - Health repair logic (delegated to parent via closures).
 - Computing operational-health summaries (handled by RalphCore).

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Summary and issue lists are passed in from RalphCore.
 - Actions are provided via closures for retry/dismiss.
 */

import SwiftUI
import RalphCore

struct OperationalStatusBannerView: View {
    let summary: WorkspaceOperationalSummary
    let onRetry: () -> Void
    let onDismiss: (() -> Void)?

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: iconName)
                .font(.system(size: 16, weight: .semibold))
                .foregroundStyle(iconColor)

            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(.primary)

                if let subtitle = subtitle {
                    Text(subtitle)
                        .font(.system(size: 11))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }

            Spacer()

            Button(action: onRetry) {
                Image(systemName: "arrow.clockwise")
                    .font(.system(size: 12, weight: .medium))
            }
            .buttonStyle(.borderless)
            .help("Retry connection")

            if let onDismiss = onDismiss {
                Button(action: onDismiss) {
                    Image(systemName: "xmark")
                        .font(.system(size: 10, weight: .medium))
                }
                .buttonStyle(.borderless)
                .help("Dismiss")
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .background(backgroundView)
        .overlay(
            Rectangle()
                .frame(height: 1)
                .foregroundStyle(borderColor.opacity(0.3)),
            alignment: .bottom
        )
    }

    @ViewBuilder
    private var backgroundView: some View {
        RoundedRectangle(cornerRadius: 0)
            .fill(
                LinearGradient(
                    colors: [
                        backgroundColor.opacity(0.15),
                        backgroundColor.opacity(0.05)
                    ],
                    startPoint: .top,
                    endPoint: .bottom
                )
            )
    }

    private var iconName: String {
        switch summary.primaryIssue?.source {
        case .cli:
            return "terminal.fill"
        case .watcher:
            return "dot.scope.display"
        case .workspacePersistence, .appPersistence:
            return "internaldrive.fill.badge.exclamationmark"
        case .workspaceRouting:
            return "arrow.triangle.2.circlepath"
        case .crashReporting:
            return "waveform.path.ecg.rectangle.fill"
        case nil:
            return "checkmark.circle.fill"
        }
    }

    private var iconColor: Color {
        switch summary.severity {
        case .error:
            return .red
        case .warning:
            return .orange
        case .info:
            return .blue
        case nil:
            return .green
        }
    }

    private var backgroundColor: Color { iconColor }
    private var borderColor: Color { iconColor }

    private var title: String {
        summary.title
    }

    private var subtitle: String? {
        summary.subtitle
    }
}

/// Smaller inline indicator for use in sidebars/toolbars
struct ConnectionStatusIndicator: View {
    let summary: WorkspaceOperationalSummary
    let onTap: () -> Void

    var body: some View {
        Button(action: onTap) {
            HStack(spacing: 6) {
                Circle()
                    .fill(statusColor)
                    .frame(width: 8, height: 8)

                Text(statusLabel)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .buttonStyle(.plain)
        .help(summary.isHealthy ? "Workspace operational health is healthy" : "Workspace has active operational issues")
    }

    private var statusColor: Color {
        switch summary.severity {
        case .error:
            return .red
        case .warning:
            return .orange
        case .info:
            return .blue
        case nil:
            return .green
        }
    }

    private var statusLabel: String {
        switch summary.severity {
        case .error:
            return "Degraded"
        case .warning:
            return "Watching"
        case .info:
            return "Starting"
        case nil:
            return "Healthy"
        }
    }
}

struct OperationalHealthSheet: View {
    let workspaceName: String
    let summary: WorkspaceOperationalSummary
    let issues: [WorkspaceOperationalIssue]
    let watcherHealth: QueueWatcherHealth
    let cliHealthStatus: CLIHealthStatus?
    let onRepair: () -> Void

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List {
                Section("Summary") {
                    LabeledContent("Workspace", value: workspaceName)
                    LabeledContent("Status", value: summary.severity?.statusText ?? "Healthy")
                    LabeledContent("Watcher", value: watcherStatusText)
                    LabeledContent("CLI", value: cliStatusText)
                }

                Section("Active Issues") {
                    if issues.isEmpty {
                        Text("No active operational issues.")
                            .foregroundStyle(.secondary)
                    } else {
                        ForEach(issues) { issue in
                            VStack(alignment: .leading, spacing: 6) {
                                Text(issue.title)
                                    .font(.headline)
                                Text(issue.message)
                                    .font(.subheadline)
                                if let recoverySuggestion = issue.recoverySuggestion {
                                    Text(recoverySuggestion)
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                            }
                            .padding(.vertical, 4)
                        }
                    }
                }
            }
            .navigationTitle("Operational Health")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Close") { dismiss() }
                }
                ToolbarItem(placement: .primaryAction) {
                    Button("Repair") { onRepair() }
                }
            }
        }
        .frame(minWidth: 520, minHeight: 360)
    }

    private var watcherStatusText: String {
        switch watcherHealth.state {
        case .idle:
            return "Idle"
        case .starting(let attempt):
            return "Starting (attempt \(attempt))"
        case .watching:
            return "Watching"
        case .degraded(_, let retryCount, _):
            return "Retrying (\(retryCount))"
        case .failed(_, let attempts):
            return "Failed after \(attempts)"
        case .stopped:
            return "Stopped"
        }
    }

    private var cliStatusText: String {
        guard let cliHealthStatus else { return "Unknown" }
        switch cliHealthStatus.availability {
        case .available:
            return "Available"
        case .unavailable:
            return "Unavailable"
        case .unknown:
            return "Unknown"
        }
    }
}
