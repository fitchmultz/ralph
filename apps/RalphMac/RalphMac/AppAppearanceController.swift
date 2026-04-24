/**
 AppAppearanceController

 Purpose:
 - Persist the selected app-wide color-scheme preference.

 Responsibilities:
 - Persist the selected app-wide color-scheme preference.
 - Apply the corresponding AppKit appearance immediately.
 - Expose the preferred SwiftUI color scheme to app scenes.

 Does not handle:
 - Settings window layout.
 - Workspace-specific appearance overrides.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/Assumptions:
 - Callers keep usage within the documented responsibilities and owning feature contracts.
 */

import AppKit
import RalphCore
import SwiftUI

enum AppColorScheme: String, CaseIterable, Codable, Identifiable {
    case system
    case light
    case dark

    var id: String { rawValue }

    var displayName: String {
        switch self {
        case .system:
            return "System"
        case .light:
            return "Light"
        case .dark:
            return "Dark"
        }
    }

    var helperText: String {
        switch self {
        case .system:
            return "Follow the current macOS appearance."
        case .light:
            return "Force Ralph into light mode."
        case .dark:
            return "Force Ralph into dark mode."
        }
    }

    var preferredColorScheme: ColorScheme? {
        switch self {
        case .system:
            return nil
        case .light:
            return .light
        case .dark:
            return .dark
        }
    }

    var nsAppearance: NSAppearance? {
        switch self {
        case .system:
            return nil
        case .light:
            return NSAppearance(named: .aqua)
        case .dark:
            return NSAppearance(named: .darkAqua)
        }
    }
}

@MainActor
final class AppAppearanceController: ObservableObject {
    static let shared = AppAppearanceController()

    @Published var selection: AppColorScheme {
        didSet {
            guard selection != oldValue else { return }
            RalphAppDefaults.userDefaults.set(selection.rawValue, forKey: Self.colorSchemeKey)
            applyAppearance()
        }
    }

    var preferredColorScheme: ColorScheme? {
        selection.preferredColorScheme
    }

    private static let colorSchemeKey = "colorScheme"

    private init() {
        let storedValue = RalphAppDefaults.userDefaults.string(forKey: Self.colorSchemeKey)
        self.selection = AppColorScheme(rawValue: storedValue ?? "") ?? .system
        applyAppearance()
    }

    func applyAppearance() {
        NSApp.appearance = selection.nsAppearance
    }
}
