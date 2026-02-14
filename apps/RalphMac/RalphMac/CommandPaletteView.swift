/**
 CommandPaletteView
 
 Responsibilities:
 - Display the command palette as a modal overlay.
 - Handle search input and display filtered results.
 - Support keyboard navigation (arrow keys, enter, escape).
 - Show command shortcuts and categories.
 
 Does not handle:
 - Command execution logic (handled by CommandPaletteViewModel).
 - Fuzzy search algorithm (handled by CommandPaletteViewModel).
 
 Invariants/assumptions:
 - Must be presented as an overlay/sheet.
 - Dismisses on Escape key or clicking outside.
 */

import SwiftUI

struct CommandPaletteView: View {
    @StateObject private var viewModel = CommandPaletteViewModel()
    @FocusState private var isSearchFieldFocused: Bool
    
    var body: some View {
        VStack(spacing: 0) {
            // Search Field
            searchField
                .padding(.horizontal, 16)
                .padding(.vertical, 12)
            
            Divider()
            
            // Results List
            resultsList
                .frame(minHeight: 100, maxHeight: 400)
        }
        .frame(width: 640)
        .background(.ultraThickMaterial)
        .clipShape(RoundedRectangle(cornerRadius: 12))
        .shadow(color: .black.opacity(0.2), radius: 20, x: 0, y: 10)
        .overlay(
            RoundedRectangle(cornerRadius: 12)
                .stroke(.separator.opacity(0.3), lineWidth: 0.5)
        )
        .onAppear {
            isSearchFieldFocused = true
            viewModel.show()
        }
        .onChange(of: viewModel.searchQuery) { _, _ in
            viewModel.validateSelection(resetToFirst: true)
        }
        // Keyboard handlers
        .onKeyPress(.upArrow) {
            viewModel.selectPrevious()
            return .handled
        }
        .onKeyPress(.downArrow) {
            viewModel.selectNext()
            return .handled
        }
        .onKeyPress(.return) {
            viewModel.executeSelectedCommand()
            return .handled
        }
        .onKeyPress(.escape) {
            viewModel.hide()
            return .handled
        }
    }
    
    // MARK: - Subviews
    
    private var searchField: some View {
        HStack(spacing: 10) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 18, weight: .medium))
                .foregroundStyle(.secondary)
            
            TextField("Type a command or search...", text: $viewModel.searchQuery)
                .font(.system(size: 18))
                .textFieldStyle(.plain)
                .focused($isSearchFieldFocused)
            
            if !viewModel.searchQuery.isEmpty {
                Button(action: { viewModel.searchQuery = "" }) {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
            }
            
            // Shortcut hint
            HStack(spacing: 4) {
                Text("ESC")
                    .font(.system(size: 11, weight: .medium))
                    .padding(.horizontal, 6)
                    .padding(.vertical, 2)
                    .background(.secondary.opacity(0.15))
                    .clipShape(.rect(cornerRadius: 4))
            }
            .foregroundStyle(.secondary)
        }
    }
    
    private var resultsList: some View {
        ScrollViewReader { proxy in
            ScrollView(.vertical) {
                if viewModel.filteredCommands.isEmpty {
                    emptyState
                } else {
                    LazyVStack(spacing: 0, pinnedViews: [.sectionHeaders]) {
                        ForEach(viewModel.groupedCommands, id: \.category) { group in
                            Section {
                                ForEach(group.commands) { command in
                                    commandRow(command)
                                }
                            } header: {
                                categoryHeader(group.category)
                            }
                        }
                    }
                }
            }
            .onChange(of: viewModel.selectedIndex) { _, newIndex in
                // Scroll to selected item
                if let command = viewModel.filteredCommands[safe: newIndex] {
                    withAnimation(.easeInOut(duration: 0.1)) {
                        proxy.scrollTo(command.id, anchor: .center)
                    }
                }
            }
            .scrollIndicators(.automatic)
        }
    }
    
    private func categoryHeader(_ category: CommandCategory) -> some View {
        HStack {
            Image(systemName: category.icon)
                .font(.caption)
                .foregroundStyle(.secondary)
            
            Text(category.rawValue)
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(.secondary)
            
            Spacer()
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 6)
        .background(.ultraThinMaterial)
    }
    
    private func commandRow(_ command: CommandPaletteCommand) -> some View {
        let isSelected = isCommandSelected(command)
        
        return Button(action: {
            viewModel.execute(command: command)
        }) {
            HStack(spacing: 12) {
                // Icon
                Image(systemName: command.icon)
                    .font(.system(size: 16))
                    .foregroundStyle(isSelected ? .white : .primary)
                    .frame(width: 24, height: 24)
                
                // Title and subtitle
                VStack(alignment: .leading, spacing: 2) {
                    Text(command.title)
                        .font(.system(size: 14, weight: .medium))
                        .foregroundStyle(isSelected ? .white : .primary)
                    
                    if let subtitle = command.subtitle {
                        Text(subtitle)
                            .font(.system(size: 11))
                            .foregroundStyle(isSelected ? .white.opacity(0.8) : .secondary)
                    }
                }
                
                Spacer()
                
                // Shortcut display
                if let shortcut = command.shortcut {
                    Text(shortcut)
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(isSelected ? .white.opacity(0.9) : .secondary)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(
                            (isSelected ? Color.white : Color.secondary)
                                .opacity(0.15)
                        )
                        .clipShape(.rect(cornerRadius: 4))
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .background(isSelected ? Color.accentColor : Color.clear)
        .id(command.id)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(command.title): \(command.subtitle ?? "")")
        .accessibilityHint("Press Enter to execute")
        .accessibilityAddTraits(isSelected ? .isSelected : [])
    }
    
    private var emptyState: some View {
        VStack(spacing: 12) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 32))
                .foregroundStyle(.secondary)
            
            Text("No commands found")
                .font(.headline)
                .foregroundStyle(.secondary)
            
            Text("Try a different search term")
                .font(.subheadline)
                .foregroundStyle(.secondary.opacity(0.7))
        }
        .frame(maxWidth: .infinity, minHeight: 150)
        .padding()
    }
    
    // MARK: - Helpers
    
    private func isCommandSelected(_ command: CommandPaletteCommand) -> Bool {
        guard let index = viewModel.filteredCommands.firstIndex(where: { $0.id == command.id }) else {
            return false
        }
        return index == viewModel.selectedIndex
    }
}

// MARK: - Array Extension

fileprivate extension Array {
    subscript(safe index: Int) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}

// MARK: - Preview

#Preview {
    ZStack {
        Color.black.opacity(0.3)
            .ignoresSafeArea()
        
        CommandPaletteView()
    }
}
