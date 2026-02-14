/**
 StringArrayEditor

 Responsibilities:
 - Display a list of editable text items.
 - Support deletion of items.
 - Allow adding new items.

 Does not handle:
 - Complex validation of item content.
 - Rich text editing.
 - Reordering items (can add if needed).
 */

import SwiftUI

/// Wrapper for string items to provide stable identity
private struct StringItem: Identifiable, Equatable {
    let id: UUID
    var value: String
    
    init(id: UUID = UUID(), value: String) {
        self.id = id
        self.value = value
    }
}

struct StringArrayEditor: View {
    @Binding var items: [String]
    var placeholder: String = "Add item..."
    
    @State private var newItemText = ""
    @State private var stringItems: [StringItem] = []
    @FocusState private var isInputFocused: Bool
    
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            itemsList
            addItemRow
        }
        .onAppear {
            syncFromItems()
        }
        .onChange(of: items) { _, _ in
            syncFromItems()
        }
    }
    
    // MARK: - Subviews
    
    private var itemsList: some View {
        Group {
            if !stringItems.isEmpty {
                VStack(spacing: 4) {
                    ForEach(Array(stringItems.enumerated()), id: \.element.id) { index, item in
                        itemRow(for: item, at: index)
                    }
                }
                .accessibilityLabel("\(stringItems.count) items in list")
            }
        }
    }
    
    private func itemRow(for item: StringItem, at index: Int) -> some View {
        HStack(spacing: 8) {
            Text("\(index + 1).")
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(width: 24, alignment: .trailing)
                .accessibilityLabel("Item \(index + 1)")
            
            TextField(placeholder, text: binding(for: item))
                .textFieldStyle(.plain)
                .accessibilityLabel("Item \(index + 1)")
            
            Button(action: { removeItem(id: item.id) }) {
                Image(systemName: "minus.circle.fill")
                    .foregroundStyle(.red.opacity(0.7))
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Remove item \(index + 1)")
            .accessibilityHint("Removes this item from the list")
        }
        .padding(.vertical, 2)
    }
    
    private var addItemRow: some View {
        HStack(spacing: 8) {
            Text("\(stringItems.count + 1).")
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(width: 24, alignment: .trailing)
            
            TextField(placeholder, text: $newItemText)
                .textFieldStyle(.plain)
                .focused($isInputFocused)
                .onSubmit {
                    addItem()
                }
                .accessibilityLabel("New item input")
                .accessibilityHint("Type an item and press Enter to add")
            
            Button(action: addItem) {
                Image(systemName: "plus.circle.fill")
            }
            .buttonStyle(.plain)
            .foregroundStyle(Color.accentColor)
            .disabled(newItemText.trimmingCharacters(in: .whitespaces).isEmpty)
            .accessibilityLabel("Add item")
            .accessibilityHint("Add the typed item to the list")
        }
        .padding(.top, stringItems.isEmpty ? 0 : 4)
    }
    
    // MARK: - Helpers
    
    private func binding(for item: StringItem) -> Binding<String> {
        Binding(
            get: {
                if let foundItem = stringItems.first(where: { $0.id == item.id }) {
                    return foundItem.value
                }
                return ""
            },
            set: { newValue in
                if let index = stringItems.firstIndex(where: { $0.id == item.id }) {
                    stringItems[index].value = newValue
                    syncToItems()
                }
            }
        )
    }
    
    private func syncFromItems() {
        // Only sync if counts differ or values differ to avoid loops
        if items.count != stringItems.count {
            stringItems = items.map { StringItem(value: $0) }
        } else {
            let valuesDiffer = zip(items, stringItems).contains { $0 != $1.value }
            if valuesDiffer {
                stringItems = items.map { StringItem(value: $0) }
            }
        }
    }
    
    private func syncToItems() {
        items = stringItems.map { $0.value }
    }
    
    private func addItem() {
        let trimmed = newItemText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        stringItems.append(StringItem(value: trimmed))
        newItemText = ""
        isInputFocused = true
        syncToItems()
        announceForAccessibility("Item \(trimmed) added")
    }
    
    private func removeItem(id: UUID) {
        stringItems.removeAll { $0.id == id }
        syncToItems()
        announceForAccessibility("Item removed")
    }

    private func announceForAccessibility(_ message: String) {
        NSAccessibility.post(
            element: NSApp as Any,
            notification: .announcementRequested,
            userInfo: [
                .announcement: message,
                .priority: NSAccessibilityPriorityLevel.high.rawValue
            ]
        )
    }
}

// Preview
#Preview {
    StringArrayEditor(
        items: .constant([
            "First item",
            "Second item",
            "Third item"
        ]),
        placeholder: "Add step..."
    )
    .padding()
}
