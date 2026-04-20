/**
 AdvancedRunnerSection

 Responsibilities:
 - Provide Advanced Runner content column with command list and filters.
 - Provide Advanced Runner detail column with argument configuration and command preview.
 - Support searching, filtering hidden commands/args, and building CLI arguments.

 Does not handle:
 - Direct CLI execution (delegated to Workspace).
 - Command palette functionality (see CommandPaletteView).

 Invariants/assumptions callers must respect:
 - Workspace is injected via @ObservedObject.
 - Commands are loaded via workspace.loadCLISpec().
 - Argument state is managed by Workspace.
 */

import SwiftUI
import RalphCore

@MainActor
struct AdvancedRunnerContentColumn: View {
    @ObservedObject var workspace: Workspace
    let navTitle: (String) -> String

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            headerSection()
                .padding(16)

            Divider()

            commandList()
        }
        .task { @MainActor in
            guard workspace.commandState.cliSpec == nil else { return }
            guard !workspace.commandState.cliSpecIsLoading else { return }
            await workspace.loadCLISpec()
        }
        .onChange(of: workspace.commandState.advancedSelectedCommandID) { _, _ in
            workspace.resetAdvancedInputs()
        }
    }

    @ViewBuilder
    private func headerSection() -> some View {
        VStack(alignment: .leading, spacing: 12) {
            WorkingDirectoryHeader(workspace: workspace)

            HStack(spacing: 16) {
                Toggle("No Color", isOn: Binding(
                    get: { workspace.commandState.advancedIncludeNoColor },
                    set: { workspace.commandState.advancedIncludeNoColor = $0 }
                ))
                    .toggleStyle(.switch)

                Toggle("Show Hidden", isOn: Binding(
                    get: { workspace.commandState.advancedShowHiddenCommands },
                    set: { workspace.commandState.advancedShowHiddenCommands = $0 }
                ))
                    .toggleStyle(.switch)

                Toggle("Hidden Args", isOn: Binding(
                    get: { workspace.commandState.advancedShowHiddenArgs },
                    set: { workspace.commandState.advancedShowHiddenArgs = $0 }
                ))
                    .toggleStyle(.switch)

                Spacer()

                if workspace.commandState.cliSpecIsLoading {
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

            if let err = workspace.commandState.cliSpecErrorMessage {
                Text(err)
                    .foregroundStyle(.red)
                    .font(.system(.caption))
                    .padding(.vertical, 4)
            }
        }
    }

    @ViewBuilder
    private func commandList() -> some View {
        let commands = filteredCommands()

        List(
            commands,
            selection: Binding(
                get: { workspace.commandState.advancedSelectedCommandID },
                set: { workspace.commandState.advancedSelectedCommandID = $0 }
            )
        ) { cmd in
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
            .tag(cmd.id)
        }
        .listStyle(.plain)
        .searchable(
            text: Binding(
                get: { workspace.commandState.advancedSearchText },
                set: { workspace.commandState.advancedSearchText = $0 }
            ),
            placement: .toolbar
        )
        .navigationTitle(navTitle("Advanced Runner"))
    }

    private func filteredCommands() -> [RalphCLICommandSpec] {
        let commands = workspace.advancedCommands()
        let query = workspace.commandState.advancedSearchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty else { return commands }

        return commands.filter { cmd in
            cmd.displayPath.localizedCaseInsensitiveContains(query)
                || (cmd.about?.localizedCaseInsensitiveContains(query) ?? false)
        }
    }
}

@MainActor
struct AdvancedRunnerDetailColumn: View {
    @ObservedObject var workspace: Workspace
    let navTitle: (String) -> String

    var body: some View {
        if let cmd = workspace.selectedAdvancedCommand() {
            commandDetailView(cmd: cmd)
        } else {
            EmptyAdvancedRunnerDetailView()
        }
    }

    @ViewBuilder
    private func commandDetailView(cmd: RalphCLICommandSpec) -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                commandHeader(cmd: cmd)

                let args = cmd.args.filter { workspace.commandState.advancedShowHiddenArgs || !$0.hidden }
                let (positional, options) = splitArgs(args)

                if !positional.isEmpty {
                    positionalArgsSection(args: positional)
                }

                if !options.isEmpty {
                    optionsSection(args: options)
                }

                commandPreviewSection(cmd: cmd)

                ConsoleView(workspace: workspace)
            }
            .padding(20)
        }
        .background(.clear)
        .navigationTitle(navTitle(cmd.name))
    }

    @ViewBuilder
    private func commandHeader(cmd: RalphCLICommandSpec) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(cmd.displayPath)
                .font(.system(.title3, design: .monospaced))
            if let about = cmd.about, !about.isEmpty {
                Text(about)
                    .foregroundStyle(.secondary)
            }
        }
    }

    @ViewBuilder
    private func positionalArgsSection(args: [RalphCLIArgSpec]) -> some View {
        GlassGroupBox(title: "Positionals") {
            VStack(alignment: .leading, spacing: 10) {
                ForEach(args, id: \.id) { arg in
                    AdvancedArgRow(workspace: workspace, arg: arg)
                }
            }
        }
    }

    @ViewBuilder
    private func optionsSection(args: [RalphCLIArgSpec]) -> some View {
        GlassGroupBox(title: "Options") {
            VStack(alignment: .leading, spacing: 10) {
                ForEach(args, id: \.id) { arg in
                    AdvancedArgRow(workspace: workspace, arg: arg)
                }
            }
        }
    }

    @ViewBuilder
    private func commandPreviewSection(cmd: RalphCLICommandSpec) -> some View {
        GlassGroupBox(title: "Command") {
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
                    .disabled(workspace.runState.isRunning)
                    .buttonStyle(GlassButtonStyle())

                    if workspace.runState.isRunning {
                        Button(action: { workspace.cancel() }) {
                            Label("Stop", systemImage: "stop.circle.fill")
                                .foregroundStyle(.red)
                        }
                        .buttonStyle(.borderless)
                    }

                    Spacer()

                    ExitStatusBadge(workspace: workspace)
                }
            }
        }
    }

    private func splitArgs(_ args: [RalphCLIArgSpec]) -> ([RalphCLIArgSpec], [RalphCLIArgSpec]) {
        let positionals = args
            .filter(\.positional)
            .sorted { ($0.index ?? Int.max) < ($1.index ?? Int.max) }
        let options = args
            .filter { !$0.positional }
            .sorted { $0.id < $1.id }
        return (positionals, options)
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

@MainActor
struct AdvancedArgRow: View {
    @ObservedObject var workspace: Workspace
    let arg: RalphCLIArgSpec

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(alignment: .firstTextBaseline) {
                Text(argDisplayName)
                    .font(.system(.body, design: .monospaced))
                    .foregroundStyle(arg.required ? Color.primary : Color.secondary)

                if arg.required {
                    Text("*")
                        .foregroundStyle(.red)
                }

                Spacer()

                argInputControl
            }

            if let help = arg.help, !help.isEmpty {
                Text(help)
                    .font(.system(.caption))
                    .foregroundStyle(.secondary)
            }
        }
    }

    @ViewBuilder
    private var argInputControl: some View {
        if arg.isCountFlag {
            Stepper(
                value: Binding(
                    get: { workspace.commandState.advancedCountValues[arg.id] ?? 0 },
                    set: { workspace.commandState.advancedCountValues[arg.id] = $0 }
                ),
                in: 0...20
            ) {
                Text("\(workspace.commandState.advancedCountValues[arg.id] ?? 0)")
                    .font(.system(.caption, design: .monospaced))
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: 220)
        } else if arg.isBooleanFlag {
            Toggle(
                "",
                isOn: Binding(
                    get: { workspace.commandState.advancedBoolValues[arg.id] ?? false },
                    set: { workspace.commandState.advancedBoolValues[arg.id] = $0 }
                )
            )
            .labelsHidden()
            .toggleStyle(.switch)
        } else if arg.takesValue {
            if arg.allowsMultipleValues {
                TextEditor(
                    text: Binding(
                        get: { workspace.commandState.advancedMultiValues[arg.id] ?? "" },
                        set: { workspace.commandState.advancedMultiValues[arg.id] = $0 }
                    )
                )
                .font(.system(.caption, design: .monospaced))
                .frame(minHeight: 48, maxHeight: 88)
            } else {
                TextField(
                    "",
                    text: Binding(
                        get: { workspace.commandState.advancedSingleValues[arg.id] ?? "" },
                        set: { workspace.commandState.advancedSingleValues[arg.id] = $0 }
                    )
                )
                .textFieldStyle(.roundedBorder)
                .font(.system(.body, design: .monospaced))
                .frame(maxWidth: 360)
            }
        }
    }

    private var argDisplayName: String {
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
}

@MainActor
struct EmptyAdvancedRunnerDetailView: View {
    var body: some View {
        VStack(spacing: 16) {
            Image(systemName: "terminal.fill")
                .font(.system(size: 48))
                .foregroundStyle(.secondary)

            Text("No Command Selected")
                .font(.headline)

            Text("Select a command from the list to configure and run it.")
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 300)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(.clear)
    }
}

@MainActor
struct ExitStatusBadge: View {
    @ObservedObject var workspace: Workspace

    var body: some View {
        if let status = workspace.runState.lastExitStatus {
            Text("Exit: \(status.code) [\(status.reason.rawValue)]")
                .font(.system(.caption, design: .monospaced))
                .foregroundStyle(status.code == 0 ? Color.secondary : Color.red)
        }
    }
}

@MainActor
struct GlassGroupBox<Content: View>: View {
    let title: String
    @ViewBuilder let content: () -> Content

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
    }
}

private extension RalphCLICommandSpec {
    var displayPath: String {
        let segments = Array(path.dropFirst())
        if segments.isEmpty {
            return name
        }
        return segments.joined(separator: " ")
    }
}
