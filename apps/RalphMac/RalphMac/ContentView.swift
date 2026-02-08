/**
 ContentView

 Responsibilities:
 - Provide a macOS SwiftUI GUI for driving `ralph` by invoking the bundled CLI via subprocess.
 - Offer both:
   - Quick actions for common workflows.
   - An "Advanced: Any Command" runner generated from `ralph __cli-spec --format json` to ensure
     every CLI command path/flag is reachable even if we haven't built bespoke screens yet.
 - Persist a small amount of user state (working directory + recents) in `UserDefaults`.

 Does not handle:
 - Implementing queue mutations, locking, migrations, runner execution, etc. Those remain in Rust.
 - Providing stdin to interactive commands. The GUI is stdout/stderr oriented and expects
   `--non-interactive` where relevant.

 Invariants/assumptions callers must respect:
 - The RalphCore client is able to locate an executable `ralph` within the app bundle.
 - The GUI runs the bundled `ralph` binary exclusively (no PATH lookup).
 */

import AppKit
import SwiftUI
import RalphCore

@MainActor
final class RalphAppModel: ObservableObject {
    private enum DefaultsKey {
        static let workingDirectoryPath = "com.mitchfultz.ralph.workingDirectoryPath"
        static let recentWorkingDirectoryPaths = "com.mitchfultz.ralph.recentWorkingDirectoryPaths"
    }

    @Published var workingDirectoryURL: URL
    @Published var recentWorkingDirectories: [URL]

    @Published var output: String = ""
    @Published var isRunning: Bool = false
    @Published var lastExitStatus: RalphCLIExitStatus?
    @Published var errorMessage: String?

    @Published var cliSpec: RalphCLISpecDocument?
    @Published var cliSpecErrorMessage: String?
    @Published var cliSpecIsLoading: Bool = false

    @Published var advancedSearchText: String = ""
    @Published var advancedShowHiddenCommands: Bool = false
    @Published var advancedShowHiddenArgs: Bool = false
    @Published var advancedIncludeNoColor: Bool = true

    @Published var advancedSelectedCommandID: String?

    // Argument value maps keyed by ArgSpec.id.
    @Published var advancedBoolValues: [String: Bool] = [:]
    @Published var advancedCountValues: [String: Int] = [:]
    @Published var advancedSingleValues: [String: String] = [:]
    @Published var advancedMultiValues: [String: String] = [:]

    private var client: RalphCLIClient?
    private var currentRun: RalphCLIRun?

    init() {
        let home = FileManager.default.homeDirectoryForCurrentUser

        let defaults = UserDefaults.standard
        let storedRecents = (defaults.array(forKey: DefaultsKey.recentWorkingDirectoryPaths) as? [String]) ?? []
        let recents = storedRecents
            .map { URL(fileURLWithPath: $0, isDirectory: true) }
            .filter { url in
                var isDir: ObjCBool = false
                return FileManager.default.fileExists(atPath: url.path, isDirectory: &isDir) && isDir.boolValue
            }

        if let stored = defaults.string(forKey: DefaultsKey.workingDirectoryPath) {
            let url = URL(fileURLWithPath: stored, isDirectory: true)
            if FileManager.default.fileExists(atPath: url.path) {
                self.workingDirectoryURL = url
            } else {
                self.workingDirectoryURL = home
            }
        } else {
            self.workingDirectoryURL = home
        }

        self.recentWorkingDirectories = recents

        do {
            self.client = try RalphCLIClient.bundled()
        } catch {
            self.errorMessage = "Failed to locate bundled ralph executable: \(error)"
        }

        Task { @MainActor in
            await loadCLISpec()
        }
    }

    func chooseWorkingDirectory() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.canCreateDirectories = true
        panel.prompt = "Choose"

        if panel.runModal() == .OK, let url = panel.url {
            setWorkingDirectory(url)
        }
    }

    func setWorkingDirectory(_ url: URL) {
        workingDirectoryURL = url

        var newRecents = recentWorkingDirectories.filter { $0.path != url.path }
        newRecents.insert(url, at: 0)
        if newRecents.count > 12 {
            newRecents = Array(newRecents.prefix(12))
        }

        recentWorkingDirectories = newRecents
        persistWorkingDirectoryState()
    }

    func selectRecentWorkingDirectory(_ url: URL) {
        setWorkingDirectory(url)
    }

    func cancel() {
        currentRun?.cancel()
    }

    func loadCLISpec() async {
        guard let client else {
            cliSpecErrorMessage = cliSpecErrorMessage ?? "CLI client not available."
            return
        }

        cliSpecIsLoading = true
        cliSpecErrorMessage = nil

        do {
            let collected = try await client.runAndCollect(
                arguments: ["--no-color", "__cli-spec", "--format", "json"],
                currentDirectoryURL: workingDirectoryURL
            )

            guard collected.status.code == 0 else {
                cliSpec = nil
                cliSpecErrorMessage = collected.stderr.isEmpty
                    ? "Failed to load CLI spec (exit \(collected.status.code))."
                    : collected.stderr
                cliSpecIsLoading = false
                return
            }

            let data = Data(collected.stdout.utf8)
            let decoded = try JSONDecoder().decode(RalphCLISpecDocument.self, from: data)
            cliSpec = decoded
        } catch {
            cliSpec = nil
            cliSpecErrorMessage = "Failed to load CLI spec: \(error)"
        }

        cliSpecIsLoading = false
    }

    func runVersion() {
        run(arguments: ["--no-color", "version"])
    }

    func runInit() {
        run(arguments: ["--no-color", "init", "--force", "--non-interactive"])
    }

    func runQueueListJSON() {
        run(arguments: ["--no-color", "queue", "list", "--format", "json"])
    }

    func run(arguments: [String]) {
        guard let client else {
            errorMessage = errorMessage ?? "CLI client not available."
            return
        }
        if isRunning {
            return
        }

        output = ""
        lastExitStatus = nil
        errorMessage = nil
        isRunning = true

        do {
            let run = try client.start(
                arguments: arguments,
                currentDirectoryURL: workingDirectoryURL
            )
            currentRun = run

            Task { @MainActor in
                for await event in run.events {
                    // Preserve source for debugging.
                    let prefix: String = (event.stream == .stdout) ? "" : "[stderr] "
                    output.append(prefix)
                    output.append(event.text)
                }

                let status = await run.waitUntilExit()
                lastExitStatus = status
                isRunning = false
                currentRun = nil
            }
        } catch {
            errorMessage = "Failed to start ralph: \(error)"
            isRunning = false
            currentRun = nil
        }
    }

    func advancedCommands() -> [RalphCLICommandSpec] {
        guard let cliSpec else { return [] }
        var out: [RalphCLICommandSpec] = []
        for sub in cliSpec.root.subcommands {
            collectCommands(sub, includeHidden: advancedShowHiddenCommands, into: &out)
        }
        return out
    }

    func selectedAdvancedCommand() -> RalphCLICommandSpec? {
        guard let id = advancedSelectedCommandID else { return nil }
        return advancedCommands().first(where: { $0.id == id })
    }

    func resetAdvancedInputs() {
        advancedBoolValues.removeAll(keepingCapacity: false)
        advancedCountValues.removeAll(keepingCapacity: false)
        advancedSingleValues.removeAll(keepingCapacity: false)
        advancedMultiValues.removeAll(keepingCapacity: false)
    }

    func buildAdvancedArguments() -> [String] {
        guard let cmd = selectedAdvancedCommand() else { return [] }
        var selections: [String: RalphCLIArgValue] = [:]

        for arg in cmd.args {
            if arg.positional {
                let raw = advancedValuesText(for: arg)
                let values = splitValuesText(raw)
                if !values.isEmpty {
                    selections[arg.id] = .values(values)
                }
                continue
            }

            if arg.isCountFlag {
                let n = advancedCountValues[arg.id] ?? 0
                if n > 0 {
                    selections[arg.id] = .count(n)
                }
                continue
            }

            if arg.isBooleanFlag {
                let present = advancedBoolValues[arg.id] ?? false
                selections[arg.id] = .flag(present)
                continue
            }

            if arg.takesValue {
                let raw = advancedValuesText(for: arg)
                let values = splitValuesText(raw)
                if !values.isEmpty {
                    selections[arg.id] = .values(values)
                }
            }
        }

        var globals: [String] = []
        if advancedIncludeNoColor {
            globals.append("--no-color")
        }
        return RalphCLIArgumentBuilder.buildArguments(
            command: cmd,
            selections: selections,
            globalArguments: globals
        )
    }

    private func advancedValuesText(for arg: RalphCLIArgSpec) -> String {
        if arg.allowsMultipleValues {
            return advancedMultiValues[arg.id] ?? ""
        }
        return advancedSingleValues[arg.id] ?? ""
    }

    private func splitValuesText(_ text: String) -> [String] {
        text
            .split(whereSeparator: \.isNewline)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
    }

    private func collectCommands(
        _ command: RalphCLICommandSpec,
        includeHidden: Bool,
        into out: inout [RalphCLICommandSpec]
    ) {
        if includeHidden || !command.hidden {
            out.append(command)
        }
        for sub in command.subcommands {
            collectCommands(sub, includeHidden: includeHidden, into: &out)
        }
    }

    private func persistWorkingDirectoryState() {
        let defaults = UserDefaults.standard
        defaults.set(workingDirectoryURL.path, forKey: DefaultsKey.workingDirectoryPath)
        defaults.set(recentWorkingDirectories.map(\.path), forKey: DefaultsKey.recentWorkingDirectoryPaths)
    }
}

struct ContentView: View {
    @StateObject private var model = RalphAppModel()

    var body: some View {
        TabView {
            quickActionsTab()
                .tabItem { Text("Quick") }

            advancedRunnerTab()
                .tabItem { Text("Advanced") }
        }
        .frame(minWidth: 920, minHeight: 640)
    }

    @ViewBuilder
    private func quickActionsTab() -> some View {
        VStack(alignment: .leading, spacing: 12) {
            workingDirectoryHeader()

            HStack(spacing: 8) {
                Button("Version") { model.runVersion() }
                Button("Init") { model.runInit() }
                Button("Queue List (JSON)") { model.runQueueListJSON() }

                Spacer()

                if model.isRunning {
                    Button("Stop") { model.cancel() }
                }

                exitStatusBadge()
            }

            consoleView()
        }
        .padding(16)
    }

    @ViewBuilder
    private func advancedRunnerTab() -> some View {
        VStack(alignment: .leading, spacing: 12) {
            workingDirectoryHeader()

            HStack(spacing: 12) {
                Toggle("No Color", isOn: $model.advancedIncludeNoColor)
                    .toggleStyle(.switch)

                Toggle("Show Hidden Commands", isOn: $model.advancedShowHiddenCommands)
                    .toggleStyle(.switch)

                Toggle("Show Hidden Args", isOn: $model.advancedShowHiddenArgs)
                    .toggleStyle(.switch)

                Spacer()

                if model.cliSpecIsLoading {
                    ProgressView()
                        .scaleEffect(0.75)
                }

                Button("Reload CLI Spec") {
                    Task { @MainActor in
                        await model.loadCLISpec()
                    }
                }
            }

            if let err = model.cliSpecErrorMessage {
                Text(err)
                    .foregroundStyle(.red)
                    .font(.system(.caption))
            }

            let commands = filteredAdvancedCommands()
            NavigationSplitView {
                List(commands, selection: $model.advancedSelectedCommandID) { cmd in
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
                .searchable(text: $model.advancedSearchText)
            } detail: {
                advancedDetailView()
            }
            .frame(minHeight: 420)
        }
        .padding(16)
        .onChange(of: model.advancedSelectedCommandID) { _, _ in
            model.resetAdvancedInputs()
        }
    }

    private func filteredAdvancedCommands() -> [RalphCLICommandSpec] {
        let commands = model.advancedCommands()
        let q = model.advancedSearchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !q.isEmpty else { return commands }

        return commands.filter { cmd in
            cmd.displayPath.localizedCaseInsensitiveContains(q)
                || (cmd.about?.localizedCaseInsensitiveContains(q) ?? false)
        }
    }

    @ViewBuilder
    private func advancedDetailView() -> some View {
        if let cmd = model.selectedAdvancedCommand() {
            VStack(alignment: .leading, spacing: 12) {
                VStack(alignment: .leading, spacing: 6) {
                    Text(cmd.displayPath)
                        .font(.system(.headline, design: .monospaced))
                    if let about = cmd.about, !about.isEmpty {
                        Text(about)
                            .foregroundStyle(.secondary)
                    }
                }

                let args = cmd.args.filter { model.advancedShowHiddenArgs || !$0.hidden }
                let (positional, options) = splitArgs(args)

                ScrollView {
                    VStack(alignment: .leading, spacing: 12) {
                        if !positional.isEmpty {
                            GroupBox("Positionals") {
                                VStack(alignment: .leading, spacing: 10) {
                                    ForEach(positional, id: \.id) { arg in
                                        advancedArgRow(arg: arg)
                                    }
                                }
                                .padding(.top, 6)
                            }
                        }

                        if !options.isEmpty {
                            GroupBox("Options") {
                                VStack(alignment: .leading, spacing: 10) {
                                    ForEach(options, id: \.id) { arg in
                                        advancedArgRow(arg: arg)
                                    }
                                }
                                .padding(.top, 6)
                            }
                        }

                        GroupBox("Command") {
                            VStack(alignment: .leading, spacing: 8) {
                                let argv = model.buildAdvancedArguments()
                                Text(shellPreview(argv: argv))
                                    .font(.system(.caption, design: .monospaced))
                                    .foregroundStyle(.secondary)
                                    .textSelection(.enabled)
                                    .frame(maxWidth: .infinity, alignment: .leading)

                                HStack {
                                    Button("Run") {
                                        let argv = model.buildAdvancedArguments()
                                        if !argv.isEmpty {
                                            model.run(arguments: argv)
                                        }
                                    }
                                    .disabled(model.isRunning)

                                    if model.isRunning {
                                        Button("Stop") { model.cancel() }
                                    }

                                    Spacer()

                                    exitStatusBadge()
                                }
                            }
                            .padding(.top, 6)
                        }

                        consoleView()
                    }
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
                            get: { model.advancedCountValues[arg.id] ?? 0 },
                            set: { model.advancedCountValues[arg.id] = $0 }
                        ),
                        in: 0...20
                    ) {
                        Text("\(model.advancedCountValues[arg.id] ?? 0)")
                            .font(.system(.caption, design: .monospaced))
                            .foregroundStyle(.secondary)
                    }
                    .frame(maxWidth: 220)
                } else if arg.isBooleanFlag {
                    Toggle(
                        "",
                        isOn: Binding(
                            get: { model.advancedBoolValues[arg.id] ?? false },
                            set: { model.advancedBoolValues[arg.id] = $0 }
                        )
                    )
                    .labelsHidden()
                    .toggleStyle(.switch)
                } else if arg.takesValue {
                    if arg.allowsMultipleValues {
                        TextEditor(
                            text: Binding(
                                get: { model.advancedMultiValues[arg.id] ?? "" },
                                set: { model.advancedMultiValues[arg.id] = $0 }
                            )
                        )
                        .font(.system(.caption, design: .monospaced))
                        .frame(minHeight: 48, maxHeight: 88)
                    } else {
                        TextField(
                            "",
                            text: Binding(
                                get: { model.advancedSingleValues[arg.id] ?? "" },
                                set: { model.advancedSingleValues[arg.id] = $0 }
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

    @ViewBuilder
    private func workingDirectoryHeader() -> some View {
        HStack(alignment: .firstTextBaseline) {
            VStack(alignment: .leading, spacing: 4) {
                Text("Working Directory")
                    .font(.headline)
                Text(model.workingDirectoryURL.path)
                    .font(.system(.body, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }

            Spacer()

            if !model.recentWorkingDirectories.isEmpty {
                Menu("Recents") {
                    ForEach(model.recentWorkingDirectories, id: \.path) { url in
                        Button(url.path) {
                            model.selectRecentWorkingDirectory(url)
                        }
                    }
                }
            }

            Button("Choose…") {
                model.chooseWorkingDirectory()
            }
        }
    }

    @ViewBuilder
    private func exitStatusBadge() -> some View {
        if let status = model.lastExitStatus {
            Text("Exit: \(status.code) [\(status.reason.rawValue)]")
                .font(.system(.caption, design: .monospaced))
                .foregroundStyle(status.code == 0 ? Color.secondary : Color.red)
        }
    }

    @ViewBuilder
    private func consoleView() -> some View {
        if let error = model.errorMessage {
            Text(error)
                .foregroundStyle(.red)
                .font(.system(.caption))
        }

        ScrollView {
            Text(model.output.isEmpty ? "(no output yet)" : model.output)
                .frame(maxWidth: .infinity, alignment: .leading)
                .font(.system(.body, design: .monospaced))
                .textSelection(.enabled)
        }
        .frame(minHeight: 240)
        .overlay(
            RoundedRectangle(cornerRadius: 8)
                .strokeBorder(Color.secondary.opacity(0.2))
        )
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
