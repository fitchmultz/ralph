/**
 RalphMacCommands

 Responsibilities:
 - Define app command menus and route them through focused workspace/window actions.

 Does not handle:
 - Workspace scene registration.
 - URL routing or window bootstrap.

 Invariants/assumptions callers must respect:
 - Commands that target a workspace require focused workspace UI actions to be registered.
 */

import SwiftUI
import RalphCore

@MainActor
struct WorkspaceCommands: Commands {
    @FocusedValue(\.workspaceWindowActions) private var workspaceWindowActions

    private func routeWindowCommand(_ command: WindowCommandRoute) {
        workspaceWindowActions?.perform(command)
    }

    var body: some Commands {
        CommandGroup(after: .newItem) {
            Divider()

            Button("Close Tab") {
                routeWindowCommand(.closeTab)
            }
            .keyboardShortcut("w", modifiers: .command)

            Button("Close Window") {
                routeWindowCommand(.closeWindow)
            }
            .keyboardShortcut("w", modifiers: [.command, .shift])
        }

        CommandMenu("Workspace") {
            Button("New Tab") {
                routeWindowCommand(.newTab)
            }
            .keyboardShortcut("t", modifiers: .command)

            Divider()

            Button("Close Tab") {
                routeWindowCommand(.closeTab)
            }

            Button("Close Window") {
                routeWindowCommand(.closeWindow)
            }

            Divider()

            Button("Next Tab") {
                routeWindowCommand(.nextTab)
            }
            .keyboardShortcut("]", modifiers: [.command, .shift])

            Button("Previous Tab") {
                routeWindowCommand(.previousTab)
            }
            .keyboardShortcut("[", modifiers: [.command, .shift])

            Divider()

            Button("Duplicate Tab") {
                routeWindowCommand(.duplicateTab)
            }
            .keyboardShortcut("d", modifiers: .command)
        }
    }
}

@MainActor
struct NavigationCommands: Commands {
    @FocusedValue(\.workspaceUIActions) private var workspaceUIActions

    private var hasFocusedWorkspace: Bool {
        workspaceUIActions != nil
    }

    var body: some Commands {
        CommandMenu("Navigation") {
            Button("Show Queue") {
                workspaceUIActions?.navigateToSection(.queue)
            }
            .keyboardShortcut("1", modifiers: .command)
            .disabled(!hasFocusedWorkspace)

            Button("Show Quick Actions") {
                workspaceUIActions?.navigateToSection(.quickActions)
            }
            .keyboardShortcut("2", modifiers: .command)
            .disabled(!hasFocusedWorkspace)

            Button("Show Run Control") {
                workspaceUIActions?.navigateToSection(.runControl)
            }
            .keyboardShortcut("3", modifiers: .command)
            .disabled(!hasFocusedWorkspace)

            Button("Show Advanced Runner") {
                workspaceUIActions?.navigateToSection(.advancedRunner)
            }
            .keyboardShortcut("4", modifiers: .command)
            .disabled(!hasFocusedWorkspace)

            Button("Show Analytics") {
                workspaceUIActions?.navigateToSection(.analytics)
            }
            .keyboardShortcut("5", modifiers: .command)
            .disabled(!hasFocusedWorkspace)

            Divider()

            Button("Toggle Sidebar") {
                workspaceUIActions?.toggleSidebar()
            }
            .keyboardShortcut("s", modifiers: [.command, .control])
            .disabled(!hasFocusedWorkspace)

            Divider()

            Button("Toggle View Mode") {
                workspaceUIActions?.toggleTaskViewMode()
            }
            .keyboardShortcut("k", modifiers: [.command, .shift])
            .disabled(!hasFocusedWorkspace)

            Button("Show Graph View") {
                workspaceUIActions?.setTaskViewMode(.graph)
            }
            .keyboardShortcut("g", modifiers: [.command, .shift])
            .disabled(!hasFocusedWorkspace)
        }
    }
}

@MainActor
struct TaskCommands: Commands {
    @FocusedValue(\.workspaceUIActions) private var workspaceUIActions

    private var hasFocusedWorkspace: Bool {
        workspaceUIActions != nil
    }

    var body: some Commands {
        CommandMenu("Task") {
            Button("New Task...") {
                workspaceUIActions?.showTaskCreation()
            }
            .keyboardShortcut("n", modifiers: [.command, .option])
            .disabled(!hasFocusedWorkspace)

            Button("Decompose Task...") {
                workspaceUIActions?.showTaskDecompose(nil)
            }
            .keyboardShortcut("d", modifiers: [.command, .option])
            .disabled(!hasFocusedWorkspace)

            Divider()

            Button("Start Work") {
                workspaceUIActions?.startWorkOnSelectedTask()
            }
            .keyboardShortcut(.return, modifiers: .command)
            .help("Change selected task status to Doing (⌘Enter)")
            .disabled(!hasFocusedWorkspace)

            Divider()

            Button("Check for CLI Updates") {
                Task { @MainActor in
                    _ = await WorkspaceManager.shared.checkForCLIUpdates()
                }
            }
        }
    }
}

@MainActor
struct CommandPaletteCommands: Commands {
    @FocusedValue(\.workspaceUIActions) private var workspaceUIActions

    private var hasFocusedWorkspace: Bool {
        workspaceUIActions != nil
    }

    var body: some Commands {
        CommandMenu("Tools") {
            Button("Command Palette...") {
                workspaceUIActions?.showCommandPalette()
            }
            .keyboardShortcut("p", modifiers: [.command, .shift])
            .disabled(!hasFocusedWorkspace)

            Button("Quick Command...") {
                workspaceUIActions?.showCommandPalette()
            }
            .keyboardShortcut("k", modifiers: .command)
            .disabled(!hasFocusedWorkspace)
        }
    }
}

struct AppHelpCommands: Commands {
    let exportLogsAction: () -> Void
    let showCrashReportsAction: () -> Void

    var body: some Commands {
        CommandGroup(replacing: .help) {
            Button("Export Logs...") {
                exportLogsAction()
            }
            .keyboardShortcut("l", modifiers: [.command, .shift])

            Button("View Crash Reports...") {
                showCrashReportsAction()
            }
            .keyboardShortcut("r", modifiers: [.command, .shift])

            Divider()

            if let docsURL = URL(string: "https://github.com/fitchmultz/ralph") {
                Link("Ralph Documentation", destination: docsURL)
            }
        }
    }
}

struct AppSettingsCommands: Commands {
    var body: some Commands {
        CommandGroup(replacing: .appSettings) {
            Button("Settings...") {
                SettingsService.showSettingsWindow(source: .commandSurface)
            }
            .keyboardShortcut(",", modifiers: .command)
        }
    }
}
