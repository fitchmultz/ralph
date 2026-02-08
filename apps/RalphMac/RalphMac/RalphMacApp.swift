/**
 RalphMacApp

 Responsibilities:
 - Define the macOS SwiftUI app entry point.
 - Present the primary window containing a thin GUI wrapper around the `ralph` CLI.

 Does not handle:
 - Any CLI execution logic (see RalphCore module).
 - Persistence of user settings beyond the lifetime of the process.

 Invariants/assumptions callers must respect:
 - The app bundle includes an executable named `ralph` placed alongside the app binary.
 */

import SwiftUI

@main
struct RalphMacApp: App {
    var body: some Scene {
        WindowGroup {
            ContentView()
        }
    }
}
