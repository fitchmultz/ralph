/**
 PreviewWorkspaceSupport

 Purpose:
 - Provide one portable workspace fixture source for SwiftUI previews.

 Responsibilities:
 - Provide one portable workspace fixture source for SwiftUI previews.
 - Eliminate hardcoded `/tmp` preview paths so preview surfaces stay portable across environments.

 Does not handle:
 - Test-only assertions or cleanup flows.
 - Sample task construction beyond returning a `Workspace`.

 Usage:
 - Call `PreviewWorkspaceSupport.makeWorkspace(label:)` from `#Preview` blocks.

 Invariants/assumptions:
 - Preview workspaces live under the system temporary directory.
 - Preview fixture directories are created eagerly and may be reused across preview refreshes.
 */

import Foundation
import RalphCore

@MainActor
enum PreviewWorkspaceSupport {
    static func makeWorkspace(label: String = #function) -> Workspace {
        let directory = workspaceURL(label: label)
        do {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        } catch {
            fatalError("Failed to create preview workspace at \(directory.path): \(error)")
        }
        return Workspace(workingDirectoryURL: directory)
    }

    static func workspaceURL(label: String = #function) -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("ralph-previews", isDirectory: true)
            .appendingPathComponent(sanitizedPathComponent(label), isDirectory: true)
    }

    private static func sanitizedPathComponent(_ raw: String) -> String {
        let replaced = raw.replacingOccurrences(
            of: "[^A-Za-z0-9._-]+",
            with: "-",
            options: .regularExpression
        )
        let trimmed = replaced.trimmingCharacters(in: CharacterSet(charactersIn: "-"))
        return trimmed.isEmpty ? "preview" : trimmed
    }
}
