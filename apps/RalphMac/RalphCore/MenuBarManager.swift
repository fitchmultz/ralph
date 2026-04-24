/**
 MenuBarManager

 Purpose:
 - Manage the menu bar extra state and preferences.

 Responsibilities:
 - Manage the menu bar extra state and preferences.
 - Provide shared state for menu bar visibility toggle.
 - Coordinate between menu bar UI and workspace state.

 Does not handle:
 - Direct menu rendering (see MenuBarContentView).
 - Window management (delegates to App via notifications).

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Must be accessed from MainActor only.
 - Uses UserDefaults for persistence of user preferences.
 */

public import Foundation
public import Combine
import SwiftUI
import OSLog

/// Manages the menu bar extra state and user preferences.
@MainActor
public final class MenuBarManager: ObservableObject {
    public static let shared = MenuBarManager()

    public static let visibilityDefaultsKey = "com.mitchfultz.ralph.menuBarExtraVisible"

    /// Whether the menu bar extra is visible (persisted to UserDefaults).
    /// SwiftUI scene insertion should use @AppStorage instead of observing this property.
    public var isMenuBarExtraVisible: Bool {
        get {
            RalphAppDefaults.userDefaults.object(forKey: Self.visibilityDefaultsKey) as? Bool ?? true
        }
        set {
            RalphAppDefaults.userDefaults.set(newValue, forKey: Self.visibilityDefaultsKey)
        }
    }
    
    /// Whether to show task status notifications (persisted to UserDefaults)
    @Published public var showStatusNotifications: Bool {
        didSet {
            RalphAppDefaults.userDefaults.set(showStatusNotifications, forKey: Self.notificationsKey)
        }
    }
    
    /// Whether to show recent tasks in the menu bar (persisted to UserDefaults)
    @Published public var showRecentTasks: Bool {
        didSet {
            RalphAppDefaults.userDefaults.set(showRecentTasks, forKey: Self.recentTasksKey)
        }
    }
    
    /// Maximum number of recent tasks to show (persisted to UserDefaults)
    @Published public var maxRecentTasks: Int {
        didSet {
            RalphAppDefaults.userDefaults.set(maxRecentTasks, forKey: Self.maxRecentTasksKey)
        }
    }
    
    // MARK: - UserDefaults Keys
    
    private static let notificationsKey = "com.mitchfultz.ralph.menuBarNotifications"
    private static let recentTasksKey = "com.mitchfultz.ralph.menuBarShowRecentTasks"
    private static let maxRecentTasksKey = "com.mitchfultz.ralph.menuBarMaxRecentTasks"
    
    // MARK: - Initialization
    
    private init() {
        // Initialize from UserDefaults with sensible defaults
        self.showStatusNotifications = RalphAppDefaults.userDefaults.object(forKey: Self.notificationsKey) as? Bool ?? false
        self.showRecentTasks = RalphAppDefaults.userDefaults.object(forKey: Self.recentTasksKey) as? Bool ?? true
        self.maxRecentTasks = RalphAppDefaults.userDefaults.object(forKey: Self.maxRecentTasksKey) as? Int ?? 5
        
        RalphLogger.shared.debug("MenuBarManager initialized", category: .lifecycle)
    }
    
    // MARK: - Public Methods
    
    /// Reset all menu bar preferences to defaults
    public func resetToDefaults() {
        isMenuBarExtraVisible = true
        showStatusNotifications = false
        showRecentTasks = true
        maxRecentTasks = 5
        
        RalphLogger.shared.info("Menu bar preferences reset to defaults", category: .lifecycle)
    }
    
    /// Toggle menu bar extra visibility
    public func toggleVisibility() {
        isMenuBarExtraVisible.toggle()
    }
}
