/**
 CommandPaletteCommand
 
 Responsibilities:
 - Define a single command available in the command palette.
 - Store metadata: title, shortcut, category, action identifier.
 - Support fuzzy matching via searchable text.
 
 Does not handle:
 - Command execution (handled by CommandPaletteViewModel).
 - UI rendering (handled by CommandPaletteView).
 */

import Foundation
import RalphCore

/// Represents a command category for grouping in the palette
enum CommandCategory: String, CaseIterable, Identifiable {
    case navigation = "Navigation"
    case task = "Task"
    case view = "View"
    case workspace = "Workspace"
    case help = "Help"
    
    var id: String { rawValue }
    
    var icon: String {
        switch self {
        case .navigation: return "arrow.right.square"
        case .task: return "checkmark.square"
        case .view: return "eye"
        case .workspace: return "folder"
        case .help: return "questionmark.circle"
        }
    }
    
    var sortOrder: Int {
        switch self {
        case .navigation: return 0
        case .task: return 1
        case .view: return 2
        case .workspace: return 3
        case .help: return 4
        }
    }
}

/// Represents a command that can be executed from the command palette
struct CommandPaletteCommand: Identifiable, Equatable {
    let id: String
    let title: String
    let subtitle: String?
    let shortcut: String?
    let category: CommandCategory
    let icon: String
    let action: CommandAction
    
    /// Searchable text for fuzzy matching (includes title, subtitle, keywords)
    var searchableText: String {
        var components = [title]
        if let subtitle = subtitle {
            components.append(subtitle)
        }
        return components.joined(separator: " ").lowercased()
    }
    
    static func == (lhs: CommandPaletteCommand, rhs: CommandPaletteCommand) -> Bool {
        lhs.id == rhs.id
    }
}

/// Defines the action to perform when a command is selected
enum CommandAction: Equatable {
    /// Navigate to a specific sidebar section
    case navigateToSection(SidebarSection)
    
    /// Toggle sidebar visibility
    case toggleSidebar
    
    /// Toggle task view mode (list/kanban/graph)
    case toggleTaskViewMode
    
    /// Set specific task view mode
    case setTaskViewMode(TaskViewMode)
    
    /// Show graph view
    case showGraphView
    
    /// Show task creation sheet
    case showTaskCreation

    /// Show task decomposition sheet
    case showTaskDecompose

    /// Start work on selected task
    case startWork
    
    /// Workspace actions
    case newTab
    case closeTab
    case duplicateTab
    case nextTab
    case previousTab
    case newWindow
    case closeWindow
    
    /// Help actions
    case showCommandPaletteHelp
}

// MARK: - Command Registry

extension CommandPaletteCommand {
    /// Returns all available commands for the palette
    static var allCommands: [CommandPaletteCommand] {
        [
            // MARK: Navigation Commands
            CommandPaletteCommand(
                id: "nav.queue",
                title: "Show Queue",
                subtitle: "View task queue",
                shortcut: "⌘1",
                category: .navigation,
                icon: "list.bullet.rectangle",
                action: .navigateToSection(.queue)
            ),
            CommandPaletteCommand(
                id: "nav.quickActions",
                title: "Show Quick Actions",
                subtitle: "Quick CLI commands",
                shortcut: "⌘2",
                category: .navigation,
                icon: "bolt.fill",
                action: .navigateToSection(.quickActions)
            ),
            CommandPaletteCommand(
                id: "nav.runControl",
                title: "Show Run Control",
                subtitle: "Task execution control",
                shortcut: "⌘3",
                category: .navigation,
                icon: "play.circle.fill",
                action: .navigateToSection(.runControl)
            ),
            CommandPaletteCommand(
                id: "nav.advancedRunner",
                title: "Show Advanced Runner",
                subtitle: "Advanced CLI runner",
                shortcut: "⌘4",
                category: .navigation,
                icon: "terminal.fill",
                action: .navigateToSection(.advancedRunner)
            ),
            CommandPaletteCommand(
                id: "nav.analytics",
                title: "Show Analytics",
                subtitle: "Task analytics and insights",
                shortcut: "⌘5",
                category: .navigation,
                icon: "chart.bar.fill",
                action: .navigateToSection(.analytics)
            ),
            
            // MARK: View Commands
            CommandPaletteCommand(
                id: "view.toggleSidebar",
                title: "Toggle Sidebar",
                subtitle: "Show or hide the sidebar",
                shortcut: "⌘⌃S",
                category: .view,
                icon: "sidebar.left",
                action: .toggleSidebar
            ),
            CommandPaletteCommand(
                id: "view.toggleViewMode",
                title: "Toggle View Mode",
                subtitle: "Cycle through list, kanban, and graph views",
                shortcut: "⌘⇧K",
                category: .view,
                icon: "rectangle.split.3x3",
                action: .toggleTaskViewMode
            ),
            CommandPaletteCommand(
                id: "view.listMode",
                title: "Switch to List View",
                subtitle: "Display tasks in list format",
                shortcut: nil,
                category: .view,
                icon: "list.bullet",
                action: .setTaskViewMode(.list)
            ),
            CommandPaletteCommand(
                id: "view.kanbanMode",
                title: "Switch to Kanban View",
                subtitle: "Display tasks in kanban board",
                shortcut: nil,
                category: .view,
                icon: "rectangle.split.3x3",
                action: .setTaskViewMode(.kanban)
            ),
            CommandPaletteCommand(
                id: "view.graphMode",
                title: "Switch to Graph View",
                subtitle: "Display task dependency graph",
                shortcut: "⌘⇧G",
                category: .view,
                icon: "point.3.connected.trianglepath.dotted",
                action: .setTaskViewMode(.graph)
            ),
            
            // MARK: Task Commands
            CommandPaletteCommand(
                id: "task.new",
                title: "New Task...",
                subtitle: "Create a new task",
                shortcut: "⌘⌥N",
                category: .task,
                icon: "plus.square",
                action: .showTaskCreation
            ),
            CommandPaletteCommand(
                id: "task.decompose",
                title: "Decompose Task...",
                subtitle: "Preview and write a task tree",
                shortcut: "⌘⌥D",
                category: .task,
                icon: "square.split.2x2",
                action: .showTaskDecompose
            ),
            CommandPaletteCommand(
                id: "task.startWork",
                title: "Start Work",
                subtitle: "Begin working on the selected task",
                shortcut: "⌘↩",
                category: .task,
                icon: "play.fill",
                action: .startWork
            ),
            
            // MARK: Workspace Commands
            CommandPaletteCommand(
                id: "workspace.newTab",
                title: "New Tab",
                subtitle: "Open a new workspace tab",
                shortcut: "⌘T",
                category: .workspace,
                icon: "plus.rectangle",
                action: .newTab
            ),
            CommandPaletteCommand(
                id: "workspace.closeTab",
                title: "Close Tab",
                subtitle: "Close the current tab",
                shortcut: "⌘W",
                category: .workspace,
                icon: "xmark.rectangle",
                action: .closeTab
            ),
            CommandPaletteCommand(
                id: "workspace.duplicateTab",
                title: "Duplicate Tab",
                subtitle: "Duplicate the current tab",
                shortcut: "⌘D",
                category: .workspace,
                icon: "doc.on.doc",
                action: .duplicateTab
            ),
            CommandPaletteCommand(
                id: "workspace.nextTab",
                title: "Next Tab",
                subtitle: "Switch to the next tab",
                shortcut: "⌘⇧]",
                category: .workspace,
                icon: "chevron.right",
                action: .nextTab
            ),
            CommandPaletteCommand(
                id: "workspace.previousTab",
                title: "Previous Tab",
                subtitle: "Switch to the previous tab",
                shortcut: "⌘⇧[",
                category: .workspace,
                icon: "chevron.left",
                action: .previousTab
            ),
            CommandPaletteCommand(
                id: "workspace.newWindow",
                title: "New Window",
                subtitle: "Open a new window",
                shortcut: "⌘⇧N",
                category: .workspace,
                icon: "plus.rectangle.on.rectangle",
                action: .newWindow
            ),
            CommandPaletteCommand(
                id: "workspace.closeWindow",
                title: "Close Window",
                subtitle: "Close the current window",
                shortcut: "⌘⇧W",
                category: .workspace,
                icon: "xmark",
                action: .closeWindow
            ),
            
            // MARK: Help Commands
            CommandPaletteCommand(
                id: "help.commandPalette",
                title: "Command Palette Help",
                subtitle: "Learn about keyboard shortcuts",
                shortcut: "⌘⇧P",
                category: .help,
                icon: "questionmark.circle",
                action: .showCommandPaletteHelp
            )
        ]
    }
}
