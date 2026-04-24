/**
 ASettingsInfra

 Purpose:
 - Define shared settings-window identity constants used across the decomposed settings scene runtime.

 Responsibilities:
 - Define shared settings-window identity constants used across the decomposed settings scene runtime.
 - Keep the root infra file as a thin facade while adjacent files own window service, diagnostics, appearance, and scene composition.

 Does not handle:
 - Settings tab content (defined in `AppSettings.swift` and companion files).
 - Settings open command wiring (defined in `SettingsService.swift`).
 - Window-service, diagnostics, or focus-anchor implementation details.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/Assumptions:
 - Callers keep usage within the documented responsibilities and owning feature contracts.
 */

import AppKit
import RalphCore
import SwiftUI

enum SettingsWindowIdentity {
    static let sceneID = "settings"
    static let windowIdentifier = "com.mitchfultz.ralph.settings-window"
    static let legacyWindowIdentifier = "com_apple_SwiftUI_Settings_window"
    static let title = "Settings"
}
