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

struct StringArrayEditor: View {
    @Binding var items: [String]
    var placeholder: String = "Add item..."
    
    @State private var newItemText = ""
    @FocusState private var isInputFocused: Bool
    
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            // Items list
            if !items.isEmpty {
                VStack(spacing: 4) {
                    ForEach(Array(items.enumerated()), id: \.offset) { index, item in
                        HStack(spacing: 8) {
                            Text("\(index + 1).")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                                .frame(width: 24, alignment: .trailing)
                            
                            TextField(placeholder, text: $items[index])
                                .textFieldStyle(.plain)
                            
                            Button(action: { removeItem(at: index) }) {
                                Image(systemName: "minus.circle.fill")
                                    .foregroundStyle(.red.opacity(0.7))
                            }
                            .buttonStyle(.plain)
                        }
                        .padding(.vertical, 2)
                    }
                }
            }
            
            // Add new item
            HStack(spacing: 8) {
                Text("\(items.count + 1).")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .frame(width: 24, alignment: .trailing)
                
                TextField(placeholder, text: $newItemText)
                    .textFieldStyle(.plain)
                    .focused($isInputFocused)
                    .onSubmit {
                        addItem()
                    }
                
                Button(action: addItem) {
                    Image(systemName: "plus.circle.fill")
                }
                .buttonStyle(.plain)
                .foregroundStyle(Color.accentColor)
                .disabled(newItemText.trimmingCharacters(in: .whitespaces).isEmpty)
            }
            .padding(.top, items.isEmpty ? 0 : 4)
        }
    }
    
    private func addItem() {
        let trimmed = newItemText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        items.append(trimmed)
        newItemText = ""
        isInputFocused = true
    }
    
    private func removeItem(at index: Int) {
        guard index >= 0 && index < items.count else { return }
        items.remove(at: index)
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
