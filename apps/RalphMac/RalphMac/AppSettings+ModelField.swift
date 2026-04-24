/**
 AppSettings+ModelField

 Purpose:
 - Host the AppKit-backed model text field used by Settings.

 Responsibilities:
 - Host the AppKit-backed model text field used by Settings.
 - Disable writing-tools and text-substitution helpers that interfere with deterministic settings entry.

 Does not handle:
 - Runner tab layout.
 - Settings persistence logic.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/Assumptions:
 - Callers keep usage within the documented responsibilities and owning feature contracts.
 */

import AppKit
import SwiftUI

@MainActor
struct SettingsModelTextField: NSViewRepresentable {
    @Binding var text: String

    func makeCoordinator() -> Coordinator {
        Coordinator(text: $text)
    }

    func makeNSView(context: Context) -> NSTextField {
        let textField = NSTextField(string: text)
        textField.placeholderString = "Model name"
        textField.delegate = context.coordinator
        textField.identifier = NSUserInterfaceItemIdentifier(SettingsAccessibilityID.modelField)
        configure(textField)
        return textField
    }

    func updateNSView(_ nsView: NSTextField, context: Context) {
        if nsView.stringValue != text {
            nsView.stringValue = text
        }
        configure(nsView)
        context.coordinator.configureFieldEditorIfNeeded(for: nsView)
    }

    private func configure(_ textField: NSTextField) {
        if #available(macOS 15.2, *) {
            textField.allowsWritingTools = false
        }
        textField.isAutomaticTextCompletionEnabled = false
        if #available(macOS 15.4, *) {
            textField.allowsWritingToolsAffordance = false
        }
    }

    @MainActor
    final class Coordinator: NSObject, NSTextFieldDelegate {
        @Binding private var text: String

        init(text: Binding<String>) {
            self._text = text
        }

        func controlTextDidBeginEditing(_ notification: Notification) {
            guard let textField = notification.object as? NSTextField else { return }
            configureFieldEditorIfNeeded(for: textField)
        }

        func controlTextDidChange(_ notification: Notification) {
            guard let textField = notification.object as? NSTextField else { return }
            configureFieldEditorIfNeeded(for: textField)
            text = textField.stringValue
        }

        func configureFieldEditorIfNeeded(for textField: NSTextField) {
            if #available(macOS 15.2, *) {
                textField.allowsWritingTools = false
            }
            textField.isAutomaticTextCompletionEnabled = false
            if #available(macOS 15.4, *) {
                textField.allowsWritingToolsAffordance = false
            }

            guard let editor = textField.currentEditor() as? NSTextView else { return }
            if #available(macOS 15.0, *) {
                editor.writingToolsBehavior = .none
            }
            editor.isContinuousSpellCheckingEnabled = false
            editor.isGrammarCheckingEnabled = false
            editor.isAutomaticQuoteSubstitutionEnabled = false
            editor.isAutomaticDashSubstitutionEnabled = false
            editor.isAutomaticTextReplacementEnabled = false
            editor.isAutomaticSpellingCorrectionEnabled = false
            editor.isAutomaticTextCompletionEnabled = false
            editor.smartInsertDeleteEnabled = false
        }
    }
}
