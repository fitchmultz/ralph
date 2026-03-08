/**
 AppDelegate

 Responsibilities:
 - Configure window behavior before SwiftUI takes over.
 - Disable automatic window tabbing globally before any windows are created.
 - Keep existing windows out of automatic tabbing mode after launch.

 Does not handle:
 - SwiftUI view hierarchy.
 - Window content management.
 - App command routing or keyboard shortcut dispatch.

 Invariants/assumptions callers must respect:
 - Must be connected via @NSApplicationDelegateAdaptor in the SwiftUI App struct.
 - applicationWillFinishLaunching is called before any windows are created.
 - Workspace window commands are routed through focused scene values in SwiftUI.
 */

import SwiftUI
import RalphCore

@MainActor
class AppDelegate: NSObject, NSApplicationDelegate {
    func applicationDidFinishLaunching(_ notification: Notification) {
        // Disable automatic window tabbing globally
        NSWindow.allowsAutomaticWindowTabbing = false
        
        // Configure any existing windows
        for window in NSApplication.shared.windows {
            window.tabbingMode = .disallowed
        }
        
        // Settings infrastructure is initialized from the SwiftUI app entry point.
    }
    
    func applicationWillFinishLaunching(_ notification: Notification) {
        // Disable tabbing before any windows are created
        NSWindow.allowsAutomaticWindowTabbing = false
    }

    func applicationWillTerminate(_ notification: Notification) {
        WorkspaceManager.shared.persistRegisteredWindowStates()
    }
}
