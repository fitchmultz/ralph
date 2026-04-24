/**
 RalphCLIExecutableLocator

 Purpose:
 - Provide a single place to resolve the on-disk `ralph` executable used by the macOS GUI.

 Responsibilities:
 - Provide a single place to resolve the on-disk `ralph` executable used by the macOS GUI.
 - Prefer the app-bundled `ralph` placed next to the app executable (Contents/MacOS/ralph).

 Does not handle:
 - Building or copying the `ralph` binary into the bundle (handled by the Xcode build phase).
 - Falling back to `PATH` lookup. If the binary isn't bundled, the GUI treats this as a configuration error.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - The GUI build step bundles an executable file named `ralph` into the app bundle.
 */

public import Foundation

public enum RalphCLIExecutableLocator {
    public enum LocatorError: Error, Equatable {
        case bundledExecutableNotFound
    }

    public static func bundledRalphExecutableURL(bundle: Bundle = .main) throws -> URL {
        if let url = bundle.url(forAuxiliaryExecutable: "ralph") {
            return url
        }

        // Fallback for situations where Bundle APIs are picky about location.
        let candidate = bundle.bundleURL
            .appendingPathComponent("Contents", isDirectory: true)
            .appendingPathComponent("MacOS", isDirectory: true)
            .appendingPathComponent("ralph", isDirectory: false)

        if FileManager.default.isExecutableFile(atPath: candidate.path) {
            return candidate
        }

        throw LocatorError.bundledExecutableNotFound
    }
}
