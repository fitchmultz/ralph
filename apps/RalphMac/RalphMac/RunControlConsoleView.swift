/**
 RunControlConsoleView

 Responsibilities:
 - Display live console output from task execution with ANSI color support
 - Auto-scroll to show latest output
 - Provide copy and clear functionality
 - Support text selection for copying snippets

 Does not handle:
 - Direct CLI communication (delegated to Workspace)
 - ANSI parsing logic (handled by Workspace.ANSISegment)

 Invariants/assumptions callers must respect:
 - Workspace is injected as ObservedObject for reactive updates
 - Output updates arrive on main thread
 */

import SwiftUI
import RalphCore

struct RunControlConsoleView: View {
    @ObservedObject var workspace: Workspace
    @State private var autoScroll = true
    @State private var scrollProxy: ScrollViewProxy?

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            // Header with controls
            HStack {
                Text("Console Output")
                    .font(.system(.caption, weight: .semibold))
                    .foregroundStyle(.secondary)

                Spacer()

                // Auto-scroll toggle
                Toggle("Auto-scroll", isOn: $autoScroll)
                    .toggleStyle(.switch)
                    .controlSize(.small)

                Divider()
                    .frame(height: 16)

                // Copy button
                Button(action: copyOutput) {
                    Image(systemName: "doc.on.doc")
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .help("Copy all output")

                // Clear button (only when not running)
                if !workspace.isRunning && !workspace.output.isEmpty {
                    Button(action: clearOutput) {
                        Image(systemName: "xmark.circle")
                    }
                    .buttonStyle(.plain)
                    .foregroundStyle(.secondary)
                    .help("Clear output")
                }
            }

            // Console content
            ScrollViewReader { proxy in
                ScrollView {
                    if workspace.attributedOutput.isEmpty {
                        // Plain text fallback
                        Text(workspace.output.isEmpty ? "(no output yet)" : workspace.output)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .font(.system(.body, design: .monospaced))
                            .textSelection(.enabled)
                            .padding(12)
                            .id("console-bottom")
                    } else {
                        // Rich attributed text
                        attributedConsoleContent()
                            .padding(12)
                            .id("console-bottom")
                    }
                }
                .frame(minHeight: 300)
                .underPageBackground(cornerRadius: 10, isEmphasized: false)
                .overlay(
                    RoundedRectangle(cornerRadius: 10, style: .continuous)
                        .strokeBorder(.separator.opacity(0.3), lineWidth: 0.5)
                )
                .onChange(of: workspace.output) { _, _ in
                    if autoScroll {
                        withAnimation(.easeOut(duration: 0.1)) {
                            proxy.scrollTo("console-bottom", anchor: .bottom)
                        }
                    }
                }
            }

            // Status bar
            HStack {
                if workspace.isRunning {
                    HStack(spacing: 6) {
                        ProgressView()
                            .scaleEffect(0.6)
                            .controlSize(.small)
                        Text("Running...")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                } else if let status = workspace.lastExitStatus {
                    HStack(spacing: 6) {
                        Image(systemName: status.code == 0 ? "checkmark.circle.fill" : "xmark.circle.fill")
                            .foregroundStyle(status.code == 0 ? .green : .red)
                        Text("Exit code: \(status.code)")
                            .font(.caption)
                            .foregroundStyle(status.code == 0 ? .green : .red)
                    }
                }

                Spacer()

                // Character count
                Text("\(workspace.output.count) chars")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }

    @ViewBuilder
    private func attributedConsoleContent() -> some View {
        // Build attributed text from segments
        // Note: In a real implementation, you might use Text concatenation
        // or NSAttributedString bridging for complex cases
        VStack(alignment: .leading, spacing: 0) {
            ForEach(workspace.attributedOutput) { segment in
                Text(segment.text)
                    .foregroundStyle(segment.color.swiftUIColor)
                    .font(.system(.body, design: .monospaced)
                        .weight(segment.isBold ? .bold : .regular)
                    )
            }
        }
        .textSelection(.enabled)
    }

    private func copyOutput() {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(workspace.output, forType: .string)
    }

    private func clearOutput() {
        workspace.output = ""
        workspace.attributedOutput = []
    }
}

#Preview {
    // Create a mock workspace for preview
    let workspace = Workspace(workingDirectoryURL: URL(fileURLWithPath: "/tmp"))
    workspace.output = "Sample console output line 1\nSample console output line 2\n"
    return RunControlConsoleView(workspace: workspace)
        .padding()
        .frame(width: 600, height: 400)
}
