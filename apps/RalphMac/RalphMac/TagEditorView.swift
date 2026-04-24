/**
 TagEditorView

 Purpose:
 - Display tags as removable chips.

 Responsibilities:
 - Display tags as removable chips.
 - Allow adding new tags via text input.
 - Prevent duplicate tags.

 Does not handle:
 - Tag validation against a predefined list.
 - Tag suggestions/autocomplete.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/Assumptions:
 - Callers keep usage within the documented responsibilities and owning feature contracts.
 */

import SwiftUI

struct TagEditorView: View {
    @Binding var tags: [String]
    
    @State private var newTagText = ""
    @FocusState private var isInputFocused: Bool
    
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            // Tag chips in a FlowLayout
            FlowLayout(spacing: 6) {
                ForEach(Array(tags.enumerated()), id: \.element) { index, tag in
                    TagChip(tag: tag) {
                        removeTag(tag)
                    }
                    .accessibilitySortPriority(Double(tags.count - index))
                }
            }
            .accessibilityLabel("Current tags: \(tags.isEmpty ? "None" : tags.joined(separator: ", "))")
            
            // Add new tag input
            HStack {
                TextField("Add tag...", text: $newTagText)
                    .textFieldStyle(.roundedBorder)
                    .focused($isInputFocused)
                    .onSubmit {
                        addTag()
                    }
                    .accessibilityLabel("New tag input")
                    .accessibilityHint("Type a tag name and press Enter to add")
                
                Button(action: addTag) {
                    Image(systemName: "plus.circle.fill")
                }
                .buttonStyle(.plain)
                .foregroundStyle(Color.accentColor)
                .disabled(newTagText.trimmingCharacters(in: .whitespaces).isEmpty)
                .accessibilityLabel("Add tag")
                .accessibilityHint("Add the typed tag to the list")
            }
        }
    }
    
    private func addTag() {
        let trimmed = newTagText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        guard !tags.contains(trimmed) else {
            newTagText = ""
            return
        }
        tags.append(trimmed)
        newTagText = ""
        isInputFocused = true
    }
    
    private func removeTag(_ tag: String) {
        tags.removeAll { $0 == tag }
    }
}

// TagChip view
struct TagChip: View {
    let tag: String
    let onRemove: () -> Void
    
    var body: some View {
        HStack(spacing: 4) {
            Text(tag)
                .font(.caption)
            
            Button(action: onRemove) {
                Image(systemName: "xmark")
                    .font(.caption2)
            }
            .buttonStyle(.plain)
            .foregroundStyle(.secondary)
            .accessibilityLabel("Remove tag \(tag)")
            .accessibilityHint("Removes this tag from the task")
        }
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .background(Color.accentColor.opacity(0.15))
        .foregroundStyle(.primary)
        .clipShape(.rect(cornerRadius: 6))
        .accessibilityElement(children: .combine)
        .accessibilityLabel("Tag: \(tag)")
        .accessibilityHint("Double click to remove")
    }
}

// FlowLayout - custom layout that wraps items like CSS flexbox
struct FlowLayout: Layout {
    var spacing: CGFloat = 8
    
    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let result = FlowResult(in: proposal.width ?? 0, subviews: subviews, spacing: spacing)
        return result.size
    }
    
    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        let result = FlowResult(in: bounds.width, subviews: subviews, spacing: spacing)
        for (index, subview) in subviews.enumerated() {
            subview.place(at: CGPoint(x: bounds.minX + result.positions[index].x,
                                      y: bounds.minY + result.positions[index].y),
                         proposal: .unspecified)
        }
    }
    
    struct FlowResult {
        var size: CGSize = .zero
        var positions: [CGPoint] = []
        
        init(in maxWidth: CGFloat, subviews: Subviews, spacing: CGFloat) {
            var x: CGFloat = 0
            var y: CGFloat = 0
            var rowHeight: CGFloat = 0
            
            for subview in subviews {
                let size = subview.sizeThatFits(.unspecified)
                
                if x + size.width > maxWidth && x > 0 {
                    x = 0
                    y += rowHeight + spacing
                    rowHeight = 0
                }
                
                positions.append(CGPoint(x: x, y: y))
                rowHeight = max(rowHeight, size.height)
                x += size.width + spacing
            }
            
            self.size = CGSize(width: maxWidth, height: y + rowHeight)
        }
    }
}

// Preview
#Preview {
    TagEditorView(tags: .constant(["swift", "ui", "macos"]))
        .padding()
}
