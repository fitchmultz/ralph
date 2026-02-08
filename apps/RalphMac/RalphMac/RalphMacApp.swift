/**
 RalphMacApp

 Responsibilities:
 - Define the macOS SwiftUI app entry point.
 - Present the primary window containing a thin GUI wrapper around the `ralph` CLI.
 - Configure native macOS window styling with glass morphism effects including
   transparent titlebar and unified toolbar appearance.

 Does not handle:
 - Any CLI execution logic (see RalphCore module).
 - Persistence of user settings beyond the lifetime of the process.

 Invariants/assumptions callers must respect:
 - The app bundle includes an executable named `ralph` placed alongside the app binary.
 - VisualEffectView is available in the same module for glass morphism backgrounds.
 */

public import SwiftUI

@main
struct RalphMacApp: App {
    var body: some Scene {
        WindowGroup {
            ContentView()
                .background(
                    // Base glass morphism background for the entire window
                    VisualEffectView(material: .windowBackground, blendingMode: .behindWindow)
                        .ignoresSafeArea()
                )
        }
        .windowStyle(.hiddenTitleBar)  // Transparent titlebar for glass effect
        .windowToolbarStyle(.unified(showsTitle: false))  // Modern toolbar
        .defaultSize(width: 1200, height: 800)  // Slightly larger default
        .defaultPosition(.center)
    }
}
