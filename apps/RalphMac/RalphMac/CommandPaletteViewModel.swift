/**
 CommandPaletteViewModel
 
 Responsibilities:
 - Manage the command palette's search query and filtered results.
 - Handle command execution via focused scene actions.
 - Track selected command index for keyboard navigation.
 - Provide fuzzy search algorithm for command matching.
 
 Does not handle:
 - UI rendering (handled by CommandPaletteView).
 - Direct view manipulation outside the focused scene contracts.
 
 Invariants/assumptions:
 - Must be used as @StateObject in the view hierarchy.
 - Commands are static; no dynamic command registration.
 */

import SwiftUI
import Combine

@MainActor
final class CommandPaletteViewModel: ObservableObject {
    // MARK: - Published Properties
    
    /// Current search query entered by user
    @Published var searchQuery: String = ""
    
    /// Currently selected command index for keyboard navigation
    @Published var selectedIndex: Int = 0
    
    /// Whether the palette is currently visible
    @Published var isVisible: Bool = false
    
    // MARK: - Computed Properties
    
    /// All available commands
    let allCommands: [CommandPaletteCommand] = CommandPaletteCommand.allCommands
    
    /// Filtered commands based on search query
    var filteredCommands: [CommandPaletteCommand] {
        let query = searchQuery.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        
        guard !query.isEmpty else {
            // Return all commands grouped by category when no search
            return allCommands.sorted { cmd1, cmd2 in
                if cmd1.category.sortOrder != cmd2.category.sortOrder {
                    return cmd1.category.sortOrder < cmd2.category.sortOrder
                }
                return cmd1.title < cmd2.title
            }
        }
        
        // Fuzzy search: score each command and filter
        let scored = allCommands.map { command -> (command: CommandPaletteCommand, score: Double) in
            let score = fuzzyMatchScore(query: query, text: command.searchableText)
            return (command, score)
        }
        
        // Filter to reasonable matches and sort by score descending
        return scored
            .filter { $0.score > 0.3 }
            .sorted { $0.score > $1.score }
            .map { $0.command }
    }
    
    /// Grouped commands by category for display
    var groupedCommands: [(category: CommandCategory, commands: [CommandPaletteCommand])] {
        let commands = filteredCommands
        
        // Group by category
        let grouped = Dictionary(grouping: commands) { $0.category }
        
        // Sort categories and return
        return grouped
            .sorted { $0.key.sortOrder < $1.key.sortOrder }
            .map { ($0.key, $0.value) }
    }
    
    // MARK: - Public Methods
    
    /// Shows the command palette
    func show() {
        searchQuery = ""
        selectedIndex = 0
        isVisible = true
    }
    
    /// Hides the command palette
    func hide() {
        isVisible = false
        searchQuery = ""
    }
    
    /// Executes the selected command
    func executeSelectedCommand(
        windowActions: WorkspaceWindowActions?,
        workspaceUIActions: WorkspaceUIActions?
    ) {
        let commands = filteredCommands
        guard selectedIndex >= 0 && selectedIndex < commands.count else { return }
        
        let command = commands[selectedIndex]
        execute(
            command: command,
            windowActions: windowActions,
            workspaceUIActions: workspaceUIActions
        )
    }
    
    /// Executes a specific command
    func execute(
        command: CommandPaletteCommand,
        windowActions: WorkspaceWindowActions?,
        workspaceUIActions: WorkspaceUIActions?
    ) {
        performAction(
            command.action,
            windowActions: windowActions,
            workspaceUIActions: workspaceUIActions
        )
        hide()
    }
    
    /// Navigate up in the list
    func selectPrevious() {
        guard !filteredCommands.isEmpty else { return }
        selectedIndex = (selectedIndex - 1 + filteredCommands.count) % filteredCommands.count
    }
    
    /// Navigate down in the list
    func selectNext() {
        guard !filteredCommands.isEmpty else { return }
        selectedIndex = (selectedIndex + 1) % filteredCommands.count
    }
    
    /// Update selected index to ensure it's within bounds after filter change
    /// - Parameter resetToFirst: If true, resets selection to the first item (useful when search changes)
    func validateSelection(resetToFirst: Bool = false) {
        let count = filteredCommands.count
        if count == 0 {
            selectedIndex = 0
        } else if resetToFirst {
            selectedIndex = 0
        } else if selectedIndex >= count {
            selectedIndex = count - 1
        }
    }
    
    // MARK: - Private Methods
    
    /// Performs the actual action via focused scene contracts.
    private func performAction(
        _ action: CommandAction,
        windowActions: WorkspaceWindowActions?,
        workspaceUIActions: WorkspaceUIActions?
    ) {
        switch action {
        case .navigateToSection(let section):
            workspaceUIActions?.navigateToSection(section)
            
        case .toggleSidebar:
            workspaceUIActions?.toggleSidebar()
            
        case .toggleTaskViewMode:
            workspaceUIActions?.toggleTaskViewMode()
            
        case .setTaskViewMode(let mode):
            workspaceUIActions?.setTaskViewMode(mode)
            
        case .showGraphView:
            workspaceUIActions?.setTaskViewMode(.graph)
            
        case .showTaskCreation:
            workspaceUIActions?.showTaskCreation()

        case .showTaskDecompose:
            workspaceUIActions?.showTaskDecompose(nil)
            
        case .startWork:
            workspaceUIActions?.startWorkOnSelectedTask()
            
        case .newWindow:
            windowActions?.perform(.newWindow)

        case .newTab:
            windowActions?.perform(.newTab)
            
        case .closeTab:
            windowActions?.perform(.closeTab)
            
        case .duplicateTab:
            windowActions?.perform(.duplicateTab)
            
        case .nextTab:
            windowActions?.perform(.nextTab)
            
        case .previousTab:
            windowActions?.perform(.previousTab)
            
        case .closeWindow:
            windowActions?.perform(.closeWindow)
            
        case .showCommandPaletteHelp:
            // Could open a help window or show an alert with shortcuts
            break
        }
    }
    
    /// Simple fuzzy matching algorithm
    /// Returns score between 0 and 1, where 1 is perfect match
    private func fuzzyMatchScore(query: String, text: String) -> Double {
        let textChars = Array(text)
        let queryChars = Array(query)
        
        var queryIndex = 0
        var textIndex = 0
        var consecutiveMatches = 0
        var totalMatchScore = 0.0
        
        while queryIndex < queryChars.count && textIndex < textChars.count {
            let queryChar = queryChars[queryIndex]
            let textChar = textChars[textIndex]
            
            if queryChar == textChar {
                // Match found
                let positionBonus = 1.0 - (Double(textIndex) / Double(textChars.count)) * 0.3
                let consecutiveBonus = min(Double(consecutiveMatches) * 0.1, 0.3)
                
                // Word boundary bonus (after space or at start)
                let isWordBoundary = textIndex == 0 || textChars[textIndex - 1] == " "
                let wordBoundaryBonus = isWordBoundary ? 0.2 : 0.0
                
                totalMatchScore += positionBonus + consecutiveBonus + wordBoundaryBonus
                consecutiveMatches += 1
                queryIndex += 1
            } else {
                consecutiveMatches = 0
            }
            textIndex += 1
        }
        
        // If not all query characters matched, return low score
        guard queryIndex == queryChars.count else {
            return 0.0
        }
        
        // Normalize score
        let normalizedScore = totalMatchScore / Double(queryChars.count)
        return min(normalizedScore, 1.0)
    }
}
