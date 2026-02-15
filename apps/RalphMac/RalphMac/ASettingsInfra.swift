/**
 ASettingsInfra

 Responsibilities:
 - Settings window infrastructure (observer, window management).
 - Named with 'A' prefix to ensure compilation before RalphMacApp.swift.
 - Provides the implementation for SettingsService.initialize().

 Does not handle:
 - Settings UI content (defined in AppSettings.swift).
 - Main app window.
 */

import SwiftUI
import AppKit
import RalphCore

// MARK: - Settings Service Extension

extension SettingsService {
    /// Initializes the settings window system.
    /// Overrides the default no-op implementation in RalphMacApp.swift
    static func initialize() {
        _ = SettingsWindowObserver.shared
    }
}

// MARK: - Settings Window Observer

/// Observer that listens for settings show notifications and displays the window
@MainActor
final class SettingsWindowObserver: NSObject {
    static let shared = SettingsWindowObserver()
    
    private var window: NSWindow?
    
    private override init() {
        super.init()
        
        // Register for settings window show notification
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(showSettingsWindow),
            name: .showRalphSettings,
            object: nil
        )
    }
    
    @objc private func showSettingsWindow() {
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
