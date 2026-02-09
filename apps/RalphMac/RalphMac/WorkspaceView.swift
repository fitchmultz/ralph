/**
 WorkspaceView

 Responsibilities:
 - Display the Ralph UI using a modern three-column NavigationSplitView layout.
 - Left sidebar: Navigation sections (Queue, Quick Actions, Advanced Runner)
 - Middle column: Content list (tasks, console output, command list)
 - Right column: Detail/inspector view (task editing, command configuration)
 - Bind to a specific Workspace instance for isolated state management.

 Does not handle:
 - Window-level tab management (see WindowView).
 - Cross-workspace operations.
 - Direct navigation state persistence (see NavigationViewModel).

 Invariants/assumptions callers must respect:
 - Workspace is injected via @StateObject or @ObservedObject.
 - NavigationViewModel manages sidebar state.
 - View updates when workspace state changes.
 */

import SwiftUI
import RalphCore

struct WorkspaceView: View {
    @StateObject var workspace: Workspace
    @StateObject var navigation: NavigationViewModel

    init(workspace: Workspace, navigation: NavigationViewModel? = nil) {
        self._workspace = StateObject(wrappedValue: workspace)
        // Use provided navigation view model or create one specific to this workspace
        let navVM = navigation ?? NavigationViewModel(workspaceID: workspace.id)
        self._navigation = StateObject(wrappedValue: navVM)
    }

    var body: some View {
        NavigationSplitView(columnVisibility: $navigation.sidebarVisibility) {
            // MARK: Column 1: Sidebar
            sidebarContent()
                .navigationSplitViewColumnWidth(min: 180, ideal: 200, max: 250)
        } content: {
            // MARK: Column 2: Content List
            contentColumn()
                .navigationSplitViewColumnWidth(min: 320, ideal: 400, max: 600)
        } detail: {
            // MARK: Column 3: Detail/Inspector
            detailColumn()
                .navigationSplitViewColumnWidth(min: 450, ideal: 550, max: .infinity)
        }
        .frame(minWidth: 1200, minHeight: 640)
        .background(.clear)
    }

    // MARK: - Sidebar Column

    @ViewBuilder
    private func sidebarContent() -> some View {
        List(SidebarSection.allCases, selection: $navigation.selectedSection) { section in
            Label(section.rawValue, systemImage: section.icon)
                .tag(section)
                .accessibilityHint("Navigate to \(section.rawValue)")
        }
        .accessibilityLabel("Main navigation")
        .listStyle(.sidebar)
        #if swift(>=5.9)
        .sidebarBackground()
        #endif
        .navigationTitle("Ralph")
    }

    // MARK: - Content Column

    @ViewBuilder
    private func contentColumn() -> some View {
        switch navigation.selectedSection {
        case .queue:
            queueContent()
        case .quickActions:
            quickActionsContent()
        case .runControl:
            runControlContent()
        case .advancedRunner:
            advancedRunnerContent()
        case .analytics:
            AnalyticsDashboardView(workspace: workspace)
        }
    }

    // MARK: - Queue Content

    @ViewBuilder
    private func queueContent() -> some View {
        VStack(spacing: 0) {
            // View mode toggle toolbar
            HStack {
                Spacer()

                Picker("View Mode", selection: $navigation.taskViewMode) {
                    ForEach(TaskViewMode.allCases, id: \.self) { mode in
                        Label(mode.rawValue, systemImage: mode.icon)
                            .tag(mode)
                    }
                }
                .pickerStyle(.segmented)
                .frame(width: 240)
                .help("Switch between List, Kanban, and Graph view (⌘⇧K)")
                .accessibilityLabel("Task view mode")
                .accessibilityHint("Switch between list, kanban board, and dependency graph views")
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)

            Divider()

            // Content based on view mode
            switch navigation.taskViewMode {
            case .list:
                TaskListView(
                    workspace: workspace,
                    selectedTaskID: $navigation.selectedTaskID
                )
            case .kanban:
                KanbanBoardView(
                    workspace: workspace,
                    selectedTaskID: $navigation.selectedTaskID
                )
            case .graph:
                DependencyGraphView(
                    workspace: workspace,
                    selectedTaskID: $navigation.selectedTaskID
                )
            }
        }
    }

    // MARK: - Detail Column

    @ViewBuilder
    private func detailColumn() -> some View {
        switch navigation.selectedSection {
        case .queue:
            if let taskID = navigation.selectedTaskID,
               let task = workspace.tasks.first(where: { $0.id == taskID }) {
                TaskDetailView(
                    workspace: workspace,
                    task: task,
                    onTaskUpdated: { updatedTask in
                        // Task was saved, refresh the task list
                        Task { @MainActor in
                            await workspace.loadTasks()
                        }
                    }
                )
            } else {
                emptyDetailView(
                    icon: "list.bullet.rectangle",
                    title: "No Task Selected",
                    message: "Select a task from the list to view and edit its details."
                )
            }

        case .quickActions:
            quickActionsDetailView()

        case .runControl:
            runControlDetailView()

        case .advancedRunner:
            advancedRunnerDetailView()
        case .analytics:
            analyticsDetailView()
        }
    }

    // MARK: - Quick Actions Content Column

    @ViewBuilder
    private func quickActionsContent() -> some View {
        VStack(alignment: .leading, spacing: 0) {
            workingDirectoryHeader()
                .padding(16)

            Divider()

            consoleView()
                .padding(16)
        }
        .contentBackground(cornerRadius: 12)
        .navigationTitle("Quick Actions")
    }

    // MARK: - Run Control Content Column

    @ViewBuilder
    private func runControlContent() -> some View {
        VStack(alignment: .leading, spacing: 0) {
            workingDirectoryHeader()
                .padding(16)

            Divider()

            // Live console output
            RunControlConsoleView(workspace: workspace)
                .padding(16)
        }
        .contentBackground(cornerRadius: 12)
        .navigationTitle("Run Control")
    }

    // MARK: - Quick Actions Detail Column

    @ViewBuilder
    private func quickActionsDetailView() -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                // Working Directory Section
                VStack(alignment: .leading, spacing: 12) {
                    Text("Working Directory")
                        .font(.headline)

                    VStack(alignment: .leading, spacing: 4) {
                        Text(workspace.name)
                            .font(.subheadline)
                        Text(workspace.workingDirectoryURL.path)
                            .font(.system(.caption, design: .monospaced))
                            .foregroundStyle(.secondary)
                            .lineLimit(2)
                    }

                    HStack {
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
                        .buttonStyle(GlassButtonStyle())
                    }
                }

                Divider()

                // Quick Commands Section
                VStack(alignment: .leading, spacing: 12) {
                    Text("Quick Commands")
                        .font(.headline)

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
                    }
                }

                Divider()

                // Status Section
                VStack(alignment: .leading, spacing: 12) {
                    Text("Status")
                        .font(.headline)

                    HStack(spacing: 16) {
                        if let status = workspace.lastExitStatus {
                            HStack(spacing: 6) {
                                Image(systemName: status.code == 0 ? "checkmark.circle.fill" : "xmark.circle.fill")
                                    .foregroundStyle(status.code == 0 ? .green : .red)
                                Text("Exit: \(status.code) [\(status.reason.rawValue)]")
                                    .font(.system(.body, design: .monospaced))
                            }
                        } else {
                            Text("No commands run yet")
                                .foregroundStyle(.secondary)
                        }

                        Spacer()
                    }
                }

                if let error = workspace.errorMessage {
                    VStack(alignment: .leading, spacing: 8) {
                        Text("Error")
                            .font(.headline)
                            .foregroundStyle(.red)

                        Text(error)
                            .foregroundStyle(.red)
                            .font(.body)
                    }
                }
            }
            .padding(20)
        }
        .background(.clear)
        .navigationTitle("Quick Actions")
    }

    // MARK: - Run Control Detail Column

    @ViewBuilder
    private func runControlDetailView() -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                // Current Task Card
                if workspace.isRunning, let taskID = workspace.currentTaskID,
                   let task = workspace.tasks.first(where: { $0.id == taskID }) {
                    currentTaskCard(task: task)
                } else if !workspace.isRunning && !workspace.executionHistory.isEmpty {
                    lastRunSummary()
                } else {
                    noExecutionView()
                }

                // Phase Progress
                if workspace.isRunning {
                    phaseProgressSection()
                }

                // Runner Configuration
                runnerConfigSection()

                // Execution Controls
                executionControlsSection()

                // Execution History
                if !workspace.executionHistory.isEmpty {
                    executionHistorySection()
                }
            }
            .padding(20)
        }
        .background(.clear)
        .navigationTitle("Execution Control")
    }

    @ViewBuilder
    private func currentTaskCard(task: RalphTask) -> some View {
        glassGroupBox("Current Task") {
            VStack(alignment: .leading, spacing: 12) {
                HStack {
                    Text(task.id)
                        .font(.system(.caption, design: .monospaced))
                        .foregroundStyle(.secondary)
                        .accessibilityLabel("Task ID: \(task.id)")

                    Spacer()

                    priorityBadge(priority: task.priority)
                }

                Text(task.title)
                    .font(.headline)
                    .lineLimit(2)

                if let description = task.description, !description.isEmpty {
                    Text(description)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(3)
                }

                HStack {
                    statusBadge(status: task.status)

                    if !task.tags.isEmpty {
                        tagChips(tags: Array(task.tags.prefix(3)))
                    }

                    Spacer()

                    // Elapsed time
                    if let startTime = workspace.executionStartTime {
                        ElapsedTimeView(startTime: startTime)
                            .font(.system(.caption, design: .monospaced))
                            .foregroundStyle(.secondary)
                            .accessibilityLabel("Elapsed time")
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func phaseProgressSection() -> some View {
        glassGroupBox("Phase Progress") {
            VStack(alignment: .leading, spacing: 16) {
                // Progress bar
                GeometryReader { geo in
                    ZStack(alignment: .leading) {
                        // Background
                        RoundedRectangle(cornerRadius: 6)
                            .fill(.quaternary.opacity(0.3))
                            .frame(height: 12)

                        // Progress fill
                        if let phase = workspace.currentPhase {
                            RoundedRectangle(cornerRadius: 6)
                                .fill(phase.color)
                                .frame(width: geo.size.width * phase.progressFraction, height: 12)
                                .animation(.easeInOut(duration: 0.3), value: phase)
                        }

                        // Phase markers
                        HStack(spacing: 0) {
                            ForEach(Workspace.ExecutionPhase.allCases, id: \.self) { phase in
                                Rectangle()
                                    .fill(.separator.opacity(0.5))
                                    .frame(width: 1, height: 12)
                                    .frame(maxWidth: .infinity, alignment: .trailing)
                            }
                        }
                    }
                }
                .frame(height: 12)
                .accessibilityElement(children: .combine)
                .accessibilityLabel("Phase progress: \(workspace.currentPhase?.displayName ?? "Not started")")

                // Phase indicators
                HStack(spacing: 0) {
                    ForEach(Workspace.ExecutionPhase.allCases, id: \.self) { phase in
                        HStack(spacing: 4) {
                            Image(systemName: phase.icon)
                                .font(.caption)
                            Text(phase.displayName)
                                .font(.caption)
                        }
                        .foregroundStyle(phase == workspace.currentPhase ? phase.color : .secondary)
                        .frame(maxWidth: .infinity)
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func runnerConfigSection() -> some View {
        glassGroupBox("Runner Configuration") {
            VStack(alignment: .leading, spacing: 8) {
                configRow(icon: "cpu", label: "Model", value: workspace.currentRunnerConfig?.model ?? "Default")
                    .accessibilityElement(children: .combine)
                    .accessibilityLabel("Model: \(workspace.currentRunnerConfig?.model ?? "Default")")
                configRow(icon: "number", label: "Max Iterations", value: workspace.currentRunnerConfig?.maxIterations.map(String.init) ?? "Auto")
                    .accessibilityElement(children: .combine)
                    .accessibilityLabel("Max Iterations: \(workspace.currentRunnerConfig?.maxIterations.map(String.init) ?? "Auto")")
            }
        }
    }

    @ViewBuilder
    private func executionControlsSection() -> some View {
        glassGroupBox("Controls") {
            VStack(spacing: 12) {
                // Primary action row
                HStack(spacing: 12) {
                    if workspace.isRunning {
                        Button(action: { workspace.cancel() }) {
                            Label("Stop", systemImage: "stop.circle.fill")
                                .foregroundStyle(.red)
                        }
                        .buttonStyle(GlassButtonStyle())
                        .accessibilityLabel("Stop execution")
                        .accessibilityHint("Cancel the current task execution")

                        if workspace.isLoopMode {
                            Button(action: { workspace.stopLoop() }) {
                                Label("Stop After Current", systemImage: "pause.circle")
                                    .foregroundStyle(.orange)
                            }
                            .buttonStyle(GlassButtonStyle())
                        }
                    } else {
                        Button(action: { workspace.runNextTask() }) {
                            Label("Run Next Task", systemImage: "play.circle.fill")
                        }
                        .buttonStyle(GlassButtonStyle())
                        .accessibilityLabel("Run next task")
                        .accessibilityHint("Starts execution of the next task in the queue")

                        Button(action: { workspace.startLoop() }) {
                            Label("Start Loop", systemImage: "repeat.circle")
                        }
                        .buttonStyle(GlassButtonStyle())
                        .disabled(workspace.nextTask() == nil)
                        .accessibilityLabel("Start task loop")
                        .accessibilityHint("Continuously run tasks until stopped")
                    }

                    Spacer()
                }

                // Loop mode indicator
                if workspace.isLoopMode {
                    HStack {
                        Image(systemName: "repeat.circle.fill")
                            .foregroundStyle(.blue)
                        Text("Loop Mode Active")
                            .font(.caption)
                            .foregroundStyle(.secondary)

                        if workspace.stopAfterCurrent {
                            Text("(Stopping after current)")
                                .font(.caption)
                                .foregroundStyle(.orange)
                        }

                        Spacer()
                    }
                }

                // Exit status
                if let status = workspace.lastExitStatus, !workspace.isRunning {
                    HStack {
                        Image(systemName: status.code == 0 ? "checkmark.circle.fill" : "xmark.circle.fill")
                            .foregroundStyle(status.code == 0 ? .green : .red)
                        Text("Exit: \(status.code)")
                            .font(.system(.caption, design: .monospaced))
                            .foregroundStyle(status.code == 0 ? .green : .red)
                        Spacer()
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func executionHistorySection() -> some View {
        glassGroupBox("Recent History") {
            VStack(alignment: .leading, spacing: 8) {
                ForEach(workspace.executionHistory.prefix(5)) { record in
                    HStack {
                        Image(systemName: recordIcon(record))
                            .foregroundStyle(recordColor(record))

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
                            Text(formatDuration(duration))
                                .font(.system(.caption, design: .monospaced))
                                .foregroundStyle(.secondary)
                        }
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func noExecutionView() -> some View {
        VStack(spacing: 16) {
            Image(systemName: "play.circle")
                .font(.system(size: 48))
                .foregroundStyle(.secondary)

            Text("No Active Execution")
                .font(.headline)

            Text("Run a task to see execution progress and live output.")
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 300)
        }
        .frame(maxWidth: .infinity, minHeight: 200)
    }

    @ViewBuilder
    private func lastRunSummary() -> some View {
        if let lastRun = workspace.executionHistory.first {
            glassGroupBox("Last Run") {
                HStack {
                    Image(systemName: recordIcon(lastRun))
                        .foregroundStyle(recordColor(lastRun))

                    if let taskID = lastRun.taskID {
                        Text(taskID)
                            .font(.system(.body, design: .monospaced))
                    }

                    Spacer()

                    if let duration = lastRun.duration {
                        Text(formatDuration(duration))
                            .font(.system(.body, design: .monospaced))
                            .foregroundStyle(.secondary)
                    }
                }
            }
        }
    }

    // MARK: - Helper Views

    @ViewBuilder
    private func priorityBadge(priority: RalphTaskPriority) -> some View {
        HStack(spacing: 4) {
            Circle()
                .fill(priorityColor(priority))
                .frame(width: 8, height: 8)
            Text(priority.displayName)
                .font(.caption)
        }
        .accessibilityLabel("Priority: \(priority.displayName)")
    }

    @ViewBuilder
    private func statusBadge(status: RalphTaskStatus) -> some View {
        HStack(spacing: 4) {
            Circle()
                .fill(statusColor(status))
                .frame(width: 8, height: 8)
            Text(status.displayName)
                .font(.caption)
        }
        .accessibilityLabel("Status: \(status.displayName)")
    }

    @ViewBuilder
    private func tagChips(tags: [String]) -> some View {
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

    @ViewBuilder
    private func configRow(icon: String, label: String, value: String) -> some View {
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
    }

    // MARK: - Helper Functions

    private func recordIcon(_ record: Workspace.ExecutionRecord) -> String {
        if record.wasCancelled {
            return "xmark.octagon.fill"
        }
        return record.success ? "checkmark.circle.fill" : "xmark.circle.fill"
    }

    private func recordColor(_ record: Workspace.ExecutionRecord) -> Color {
        if record.wasCancelled {
            return .orange
        }
        return record.success ? .green : .red
    }

    private func formatDuration(_ duration: TimeInterval) -> String {
        if duration < 60 {
            return String(format: "%.0fs", duration)
        } else {
            let minutes = Int(duration) / 60
            let seconds = Int(duration) % 60
            return String(format: "%d:%02d", minutes, seconds)
        }
    }

    private func priorityColor(_ priority: RalphTaskPriority) -> Color {
        switch priority {
        case .critical: return .red
        case .high: return .orange
        case .medium: return .yellow
        case .low: return .green
        }
    }

    private func statusColor(_ status: RalphTaskStatus) -> Color {
        switch status {
        case .draft: return .gray
        case .todo: return .blue
        case .doing: return .orange
        case .done: return .green
        case .rejected: return .red
        }
    }

    // MARK: - Advanced Runner Content Column

    @ViewBuilder
    private func advancedRunnerContent() -> some View {
        VStack(alignment: .leading, spacing: 0) {
            // Header with controls
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

            // Command list
            let commands = filteredAdvancedCommands()
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
                .tag(cmd.id)
            }
            .listStyle(.plain)
            .searchable(text: $workspace.advancedSearchText, placement: .toolbar)
            .navigationTitle("Commands")
        }
        .onChange(of: workspace.advancedSelectedCommandID) { _, _ in
            workspace.resetAdvancedInputs()
        }
    }

    // MARK: - Advanced Runner Detail Column

    @ViewBuilder
    private func advancedRunnerDetailView() -> some View {
        if let cmd = workspace.selectedAdvancedCommand() {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    // Command Header
                    VStack(alignment: .leading, spacing: 6) {
                        Text(cmd.displayPath)
                            .font(.system(.title3, design: .monospaced))
                        if let about = cmd.about, !about.isEmpty {
                            Text(about)
                                .foregroundStyle(.secondary)
                        }
                    }

                    let args = cmd.args.filter { workspace.advancedShowHiddenArgs || !$0.hidden }
                    let (positional, options) = splitArgs(args)

                    // Positional Arguments
                    if !positional.isEmpty {
                        glassGroupBox("Positionals") {
                            VStack(alignment: .leading, spacing: 10) {
                                ForEach(positional, id: \.id) { arg in
                                    advancedArgRow(arg: arg)
                                }
                            }
                        }
                    }

                    // Options
                    if !options.isEmpty {
                        glassGroupBox("Options") {
                            VStack(alignment: .leading, spacing: 10) {
                                ForEach(options, id: \.id) { arg in
                                    advancedArgRow(arg: arg)
                                }
                            }
                        }
                    }

                    // Command Preview and Run
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

                    // Console Output
                    consoleView()
                }
                .padding(20)
            }
            .background(.clear)
            .navigationTitle(cmd.name)
        } else {
            emptyDetailView(
                icon: "terminal.fill",
                title: "No Command Selected",
                message: "Select a command from the list to configure and run it."
            )
        }
    }

    // MARK: - Analytics Detail Column

    @ViewBuilder
    private func analyticsDetailView() -> some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                if let summary = workspace.analyticsData.productivitySummary {
                    glassGroupBox("Productivity Summary") {
                        VStack(alignment: .leading, spacing: 8) {
                            DetailRow(label: "Total Completed", value: "\(summary.totalCompleted)")
                            DetailRow(label: "Current Streak", value: "\(summary.currentStreak) days")
                            DetailRow(label: "Longest Streak", value: "\(summary.longestStreak) days")
                            
                            if let nextMilestone = summary.nextMilestone {
                                DetailRow(label: "Next Milestone", value: "\(nextMilestone) tasks")
                            }
                        }
                    }
                    
                    if !summary.milestones.isEmpty {
                        glassGroupBox("Milestones Achieved") {
                            VStack(alignment: .leading, spacing: 6) {
                                ForEach(summary.milestones.prefix(5), id: \.threshold) { milestone in
                                    HStack {
                                        Image(systemName: milestone.celebrated ? "checkmark.circle.fill" : "circle")
                                            .foregroundStyle(milestone.celebrated ? .green : .secondary)
                                        Text("\(milestone.threshold) tasks")
                                        Spacer()
                                        Text(String(milestone.achievedAt.prefix(10)))
                                            .font(.caption)
                                            .foregroundStyle(.secondary)
                                    }
                                    .font(.caption)
                                }
                            }
                        }
                    }
                } else {
                    emptyDetailView(
                        icon: "chart.bar",
                        title: "No Analytics Data",
                        message: "Select a time range and refresh to load analytics."
                    )
                }
            }
            .padding(20)
        }
        .background(.clear)
        .navigationTitle("Analytics Details")
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

    // MARK: - Common UI Components

    @ViewBuilder
    private func workingDirectoryHeader() -> some View {
        HStack(alignment: .firstTextBaseline) {
            VStack(alignment: .leading, spacing: 4) {
                Text(workspace.name)
                    .font(.headline)
                    .accessibilityLabel("Workspace: \(workspace.name)")
                Text(workspace.workingDirectoryURL.path)
                    .font(.system(.body, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
                    .accessibilityLabel("Working directory: \(workspace.workingDirectoryURL.path)")
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
            .frame(minHeight: 200)
            .underPageBackground(cornerRadius: 10, isEmphasized: false)
            .overlay(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .strokeBorder(.separator.opacity(0.3), lineWidth: 0.5)
            )
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

    @ViewBuilder
    private func emptyDetailView(icon: String, title: String, message: String) -> some View {
        VStack(spacing: 16) {
            Image(systemName: icon)
                .font(.system(size: 48))
                .foregroundStyle(.secondary)
                .accessibilityHidden(true)

            Text(title)
                .font(.headline)

            Text(message)
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 300)
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(title). \(message)")
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(.clear)
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

    private func actionButton(_ title: String, icon: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Label(title, systemImage: icon)
        }
        .buttonStyle(GlassButtonStyle())
        .accessibilityLabel("\(title)")
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
