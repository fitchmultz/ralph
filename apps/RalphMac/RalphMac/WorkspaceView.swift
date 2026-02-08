/**
 WorkspaceView

 Responsibilities:
 - Display the Ralph UI for a single workspace (Quick actions + Advanced runner).
 - Bind to a specific Workspace instance for isolated state management.
 - Provide working directory header with visual project identification.

 Does not handle:
 - Window-level tab management (see WindowView).
 - Cross-workspace operations.

 Invariants/assumptions callers must respect:
 - Workspace is injected via @StateObject or @ObservedObject.
 - View updates when workspace state changes.
 */

import SwiftUI
import RalphCore

struct WorkspaceView: View {
    @StateObject var workspace: Workspace

    var body: some View {
        TabView {
            TaskListView(workspace: workspace)
                .tabItem {
                    Label("Tasks", systemImage: "list.bullet.rectangle")
                }

            quickActionsTab()
                .tabItem {
                    Label("Quick", systemImage: "bolt.fill")
                }

            advancedRunnerTab()
                .tabItem {
                    Label("Advanced", systemImage: "terminal.fill")
                }
        }
        .frame(minWidth: 920, minHeight: 640)
        .background(.clear)
    }

    // MARK: - Quick Actions Tab

    @ViewBuilder
    private func quickActionsTab() -> some View {
        VStack(alignment: .leading, spacing: 16) {
            workingDirectoryHeader()
                .padding(.horizontal, 16)
                .padding(.top, 16)

            HStack(spacing: 12) {
                actionButton("Version", icon: "info.circle.fill", action: { workspace.runVersion() })
                actionButton("Init", icon: "folder.badge.plus", action: { workspace.runInit() })

                Spacer()

                if workspace.isRunning {
                    Button(action: { workspace.cancel() }) {
                        Label("Stop", systemImage: "stop.circle.fill")
                            .foregroundStyle(.red)
                    }
                    .buttonStyle(.borderless)
                }

                exitStatusBadge()
            }
            .padding(.horizontal, 16)

            consoleView()
                .padding(.horizontal, 16)
                .padding(.bottom, 16)
        }
        .contentBackground(cornerRadius: 12)
    }

    private func actionButton(_ title: String, icon: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Label(title, systemImage: icon)
        }
        .buttonStyle(GlassButtonStyle())
    }

    // MARK: - Advanced Runner Tab

    @ViewBuilder
    private func advancedRunnerTab() -> some View {
        VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: 12) {
                workingDirectoryHeader()

                HStack(spacing: 16) {
                    Toggle("No Color", isOn: $workspace.advancedIncludeNoColor)
                        .toggleStyle(.switch)

                    Toggle("Show Hidden", isOn: $workspace.advancedShowHiddenCommands)
                        .toggleStyle(.switch)

                    Toggle("Hidden Args", isOn: $workspace.advancedShowHiddenArgs)
                        .toggleStyle(.switch)

                    Spacer()

                    if workspace.cliSpecIsLoading {
                        ProgressView()
                            .scaleEffect(0.75)
                            .controlSize(.small)
                    }

                    Button(action: {
                        Task { @MainActor in
                            await workspace.loadCLISpec()
                        }
                    }) {
                        Label("Reload", systemImage: "arrow.clockwise")
                    }
                    .buttonStyle(GlassButtonStyle())
                }

                if let err = workspace.cliSpecErrorMessage {
                    Text(err)
                        .foregroundStyle(.red)
                        .font(.system(.caption))
                        .padding(.vertical, 4)
                }
            }
            .padding(16)
            .background(.clear)

            Divider()

            let commands = filteredAdvancedCommands()
            NavigationSplitView {
                List(commands, selection: $workspace.advancedSelectedCommandID) { cmd in
                    VStack(alignment: .leading, spacing: 2) {
                        Text(cmd.displayPath)
                            .font(.system(.body, design: .monospaced))
                        if let about = cmd.about, !about.isEmpty {
                            Text(about)
                                .font(.system(.caption))
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                        }
                    }
                }
                .searchable(text: $workspace.advancedSearchText)
                .sidebarBackground()
            } detail: {
                advancedDetailView()
                    .contentBackground()
            }
            .frame(minHeight: 420)
        }
        .onChange(of: workspace.advancedSelectedCommandID) { _, _ in
            workspace.resetAdvancedInputs()
        }
    }

    private func filteredAdvancedCommands() -> [RalphCLICommandSpec] {
        let commands = workspace.advancedCommands()
        let q = workspace.advancedSearchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !q.isEmpty else { return commands }

        return commands.filter { cmd in
            cmd.displayPath.localizedCaseInsensitiveContains(q)
                || (cmd.about?.localizedCaseInsensitiveContains(q) ?? false)
        }
    }

    @ViewBuilder
    private func advancedDetailView() -> some View {
        if let cmd = workspace.selectedAdvancedCommand() {
            VStack(alignment: .leading, spacing: 12) {
                VStack(alignment: .leading, spacing: 6) {
                    Text(cmd.displayPath)
                        .font(.system(.headline, design: .monospaced))
                    if let about = cmd.about, !about.isEmpty {
                        Text(about)
                            .foregroundStyle(.secondary)
                    }
                }

                let args = cmd.args.filter { workspace.advancedShowHiddenArgs || !$0.hidden }
                let (positional, options) = splitArgs(args)

                ScrollView {
                    VStack(alignment: .leading, spacing: 16) {
                        if !positional.isEmpty {
                            glassGroupBox("Positionals") {
                                VStack(alignment: .leading, spacing: 10) {
                                    ForEach(positional, id: \.id) { arg in
                                        advancedArgRow(arg: arg)
                                    }
                                }
                            }
                        }

                        if !options.isEmpty {
                            glassGroupBox("Options") {
                                VStack(alignment: .leading, spacing: 10) {
                                    ForEach(options, id: \.id) { arg in
                                        advancedArgRow(arg: arg)
                                    }
                                }
                            }
                        }

                        glassGroupBox("Command") {
                            VStack(alignment: .leading, spacing: 8) {
                                let argv = workspace.buildAdvancedArguments()
                                Text(shellPreview(argv: argv))
                                    .font(.system(.caption, design: .monospaced))
                                    .foregroundStyle(.secondary)
                                    .textSelection(.enabled)
                                    .frame(maxWidth: .infinity, alignment: .leading)

                                HStack {
                                    Button("Run") {
                                        let argv = workspace.buildAdvancedArguments()
                                        if !argv.isEmpty {
                                            workspace.run(arguments: argv)
                                        }
                                    }
                                    .disabled(workspace.isRunning)
                                    .buttonStyle(GlassButtonStyle())

                                    if workspace.isRunning {
                                        Button(action: { workspace.cancel() }) {
                                            Label("Stop", systemImage: "stop.circle.fill")
                                                .foregroundStyle(.red)
                                        }
                                        .buttonStyle(.borderless)
                                    }

                                    Spacer()

                                    exitStatusBadge()
                                }
                            }
                        }

                        consoleView()
                    }
                    .padding(.horizontal, 4)
                }
            }
        } else {
            VStack(alignment: .leading, spacing: 8) {
                Text("Select a command")
                    .font(.headline)
                Text("The Advanced runner is generated from `ralph __cli-spec --format json`.")
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
        }
    }

    @ViewBuilder
    private func advancedArgRow(arg: RalphCLIArgSpec) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline) {
                Text(argDisplayName(arg))
                    .font(.system(.body, design: .monospaced))
                    .foregroundStyle(arg.required ? Color.primary : Color.secondary)

                if arg.required {
                    Text("*")
                        .foregroundStyle(.red)
                }

                Spacer()

                if arg.isCountFlag {
                    Stepper(
                        value: Binding(
                            get: { workspace.advancedCountValues[arg.id] ?? 0 },
                            set: { workspace.advancedCountValues[arg.id] = $0 }
                        ),
                        in: 0...20
                    ) {
                        Text("\(workspace.advancedCountValues[arg.id] ?? 0)")
                            .font(.system(.caption, design: .monospaced))
                            .foregroundStyle(.secondary)
                    }
                    .frame(maxWidth: 220)
                } else if arg.isBooleanFlag {
                    Toggle(
                        "",
                        isOn: Binding(
                            get: { workspace.advancedBoolValues[arg.id] ?? false },
                            set: { workspace.advancedBoolValues[arg.id] = $0 }
                        )
                    )
                    .labelsHidden()
                    .toggleStyle(.switch)
                } else if arg.takesValue {
                    if arg.allowsMultipleValues {
                        TextEditor(
                            text: Binding(
                                get: { workspace.advancedMultiValues[arg.id] ?? "" },
                                set: { workspace.advancedMultiValues[arg.id] = $0 }
                            )
                        )
                        .font(.system(.caption, design: .monospaced))
                        .frame(minHeight: 48, maxHeight: 88)
                    } else {
                        TextField(
                            "",
                            text: Binding(
                                get: { workspace.advancedSingleValues[arg.id] ?? "" },
                                set: { workspace.advancedSingleValues[arg.id] = $0 }
                            )
                        )
                        .textFieldStyle(.roundedBorder)
                        .font(.system(.body, design: .monospaced))
                        .frame(maxWidth: 360)
                    }
                }
            }

            if let help = arg.help, !help.isEmpty {
                Text(help)
                    .font(.system(.caption))
                    .foregroundStyle(.secondary)
            }
        }
    }

    // MARK: - Common UI Components

    @ViewBuilder
    private func workingDirectoryHeader() -> some View {
        HStack(alignment: .firstTextBaseline) {
            VStack(alignment: .leading, spacing: 4) {
                Text(workspace.name)
                    .font(.headline)
                Text(workspace.workingDirectoryURL.path)
                    .font(.system(.body, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }

            Spacer()

            if !workspace.recentWorkingDirectories.isEmpty {
                Menu("Recents") {
                    ForEach(workspace.recentWorkingDirectories, id: \.path) { url in
                        Button(url.path) {
                            workspace.selectRecentWorkingDirectory(url)
                        }
                    }
                }
            }

            Button("Choose…") {
                workspace.chooseWorkingDirectory()
            }
        }
    }

    @ViewBuilder
    private func exitStatusBadge() -> some View {
        if let status = workspace.lastExitStatus {
            Text("Exit: \(status.code) [\(status.reason.rawValue)]")
                .font(.system(.caption, design: .monospaced))
                .foregroundStyle(status.code == 0 ? Color.secondary : Color.red)
        }
    }

    @ViewBuilder
    private func consoleView() -> some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("Console Output")
                    .font(.system(.caption, weight: .semibold))
                    .foregroundStyle(.secondary)

                Spacer()

                if let error = workspace.errorMessage {
                    Text(error)
                        .foregroundStyle(.red)
                        .font(.system(.caption))
                }
            }

            ScrollView {
                Text(workspace.output.isEmpty ? "(no output yet)" : workspace.output)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .font(.system(.body, design: .monospaced))
                    .textSelection(.enabled)
                    .padding(12)
            }
            .frame(minHeight: 240)
            .underPageBackground(cornerRadius: 10, isEmphasized: false)
            .overlay(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .strokeBorder(.separator.opacity(0.3), lineWidth: 0.5)
            )
        }
    }

    private func glassGroupBox<Content: View>(_ title: String, @ViewBuilder content: () -> Content) -> some View {
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
    }

    // MARK: - Helpers

    private func splitArgs(_ args: [RalphCLIArgSpec]) -> ([RalphCLIArgSpec], [RalphCLIArgSpec]) {
        let positionals = args
            .filter(\.positional)
            .sorted { ($0.index ?? Int.max) < ($1.index ?? Int.max) }
        let options = args
            .filter { !$0.positional }
            .sorted { $0.id < $1.id }
        return (positionals, options)
    }

    private func argDisplayName(_ arg: RalphCLIArgSpec) -> String {
        if arg.positional {
            let idx = arg.index.map { "#\($0)" } ?? ""
            return "<\(arg.id)>\(idx.isEmpty ? "" : " \(idx)")"
        }

        var parts: [String] = []
        if let long = arg.long {
            parts.append("--\(long)")
        }
        if let short = arg.short, !short.isEmpty {
            parts.append("-\(short)")
        }
        if parts.isEmpty {
            return arg.id
        }
        return parts.joined(separator: " ")
    }

    private func shellPreview(argv: [String]) -> String {
        guard !argv.isEmpty else { return "" }
        return (["ralph"] + argv).map(shellEscape).joined(separator: " ")
    }

    private func shellEscape(_ s: String) -> String {
        let allowed = CharacterSet.alphanumerics
            .union(CharacterSet(charactersIn: "._/-=:"))
        if s.unicodeScalars.allSatisfy({ allowed.contains($0) }) {
            return s
        }
        return "'" + s.replacingOccurrences(of: "'", with: "'\"'\"'") + "'"
    }
}

private extension RalphCLICommandSpec {
    var displayPath: String {
        let segs = Array(path.dropFirst())
        if segs.isEmpty {
            return name
        }
        return segs.joined(separator: " ")
    }
}
