/**
 ASettingsInfra

 Responsibilities:
 - Settings window infrastructure (window management and reuse).
 - Named with 'A' prefix to ensure compilation before RalphMacApp.swift.
 - Provides the implementation for SettingsService APIs.

 Does not handle:
 - Settings UI content (defined in AppSettings.swift).
 - Main app window.
 */

import SwiftUI
import AppKit
import RalphCore

// MARK: - Settings Window Controller

@MainActor
final class SettingsWindowController {
    static let shared = SettingsWindowController()
    
    private var window: NSWindow?
    
    private init() {}

    func show() {
        if let existingWindow = window {
            existingWindow.makeKeyAndOrderFront(nil)
            NSApp.activate(ignoringOtherApps: true)
            return
        }
        
        // Create the settings content
        let settingsView = SettingsContentContainer()
            .frame(minWidth: 500, minHeight: 400)
        
        // Create hosting controller
        let hostingController = NSHostingController(rootView: settingsView)
        
        // Create window
        let newWindow = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 550, height: 450),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        newWindow.title = "Ralph Settings"
        newWindow.contentViewController = hostingController
        newWindow.center()
        newWindow.isReleasedWhenClosed = false
        
        // Store reference
        window = newWindow
        
        // Show window
        newWindow.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }
}
