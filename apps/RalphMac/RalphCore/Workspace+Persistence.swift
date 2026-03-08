/**
 Workspace+Persistence

 Responsibilities:
 - Persist workspace identity and recent-directory state in `RalphAppDefaults`.
 - Resolve queue-file paths from the active working directory.
 - Handle working-directory changes and associated watcher/config refreshes.

 Does not handle:
 - Queue mutation flows.
 - CLI subprocess execution.
 - Error recovery presentation.

 Invariants/assumptions callers must respect:
 - Persistence keys remain namespaced by `Workspace.id`.
 - Working-directory changes must flow through this extension so recents and watchers stay in sync.
 - Queue-file resolution prefers `.ralph/queue.jsonc` when both formats are absent.
 */

public import Foundation
import AppKit

public extension Workspace {
    func defaultsKey(_ suffix: String) -> String {
        "com.mitchfultz.ralph.workspace.\(id.uuidString).\(suffix)"
    }

    /// True when the workspace has a queue file the app can read/write.
    var hasRalphQueueFile: Bool {
        Self.existingQueueFileURL(in: workingDirectoryURL) != nil
    }

    /// Preferred project name for UI labels/tabs.
    ///
    /// Uses the working directory leaf name when available so titles follow the
    /// actual project path even if a stale persisted workspace name exists.
    var projectDisplayName: String {
        let pathName = workingDirectoryURL.standardizedFileURL.lastPathComponent
            .trimmingCharacters(in: .whitespacesAndNewlines)
        if !pathName.isEmpty, pathName != "/" {
            return pathName
        }

        let storedName = name.trimmingCharacters(in: .whitespacesAndNewlines)
        if !storedName.isEmpty {
            return storedName
        }

        return "workspace"
    }

    var queueFileURL: URL {
        Self.preferredQueueFileURL(in: workingDirectoryURL)
    }

    static func existingQueueFileURL(in workingDirectoryURL: URL) -> URL? {
        for fileName in ["queue.jsonc", "queue.json"] {
            let candidate = workingDirectoryURL.appendingPathComponent(".ralph/\(fileName)", isDirectory: false)
            if FileManager.default.fileExists(atPath: candidate.path) {
                return candidate
            }
        }
        return nil
    }

    static func preferredQueueFileURL(in workingDirectoryURL: URL) -> URL {
        existingQueueFileURL(in: workingDirectoryURL)
            ?? workingDirectoryURL.appendingPathComponent(".ralph/queue.jsonc", isDirectory: false)
    }

    func loadState() {
        let defaults = RalphAppDefaults.userDefaults

        if let stored = defaults.array(forKey: defaultsKey("recentPaths")) as? [String] {
            recentWorkingDirectories = stored
                .map { URL(fileURLWithPath: $0, isDirectory: true) }
                .filter { url in
                    var isDir: ObjCBool = false
                    return FileManager.default.fileExists(atPath: url.path, isDirectory: &isDir) && isDir.boolValue
                }
        }

        if let stored = defaults.string(forKey: defaultsKey("workingPath")) {
            let url = URL(fileURLWithPath: stored, isDirectory: true)
            var isDirectory: ObjCBool = false
            if FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory),
               isDirectory.boolValue {
                workingDirectoryURL = url
            }
        }

        if let storedName = defaults.string(forKey: defaultsKey("name")) {
            name = storedName
        }
    }

    func persistState() {
        let defaults = RalphAppDefaults.userDefaults
        defaults.set(workingDirectoryURL.path, forKey: defaultsKey("workingPath"))
        defaults.set(recentWorkingDirectories.map(\.path), forKey: defaultsKey("recentPaths"))
        defaults.set(name, forKey: defaultsKey("name"))
    }

    func setWorkingDirectory(_ url: URL) {
        workingDirectoryURL = url
        name = url.lastPathComponent

        var newRecents = recentWorkingDirectories.filter { $0.path != url.path }
        newRecents.insert(url, at: 0)
        if newRecents.count > 12 {
            newRecents = Array(newRecents.prefix(12))
        }
        recentWorkingDirectories = newRecents

        persistState()
        startFileWatching()
        lastTasksSnapshot.removeAll()

        if client != nil {
            Task { @MainActor [weak self] in
                await self?.loadRunnerConfiguration(retryConfiguration: .minimal)
            }
        }
    }

    func chooseWorkingDirectory() {
        let panel = NSOpenPanel()
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.allowsMultipleSelection = false
        panel.canCreateDirectories = true
        panel.prompt = "Choose"

        if panel.runModal() == .OK, let url = panel.url {
            setWorkingDirectory(url)
        }
    }

    func selectRecentWorkingDirectory(_ url: URL) {
        setWorkingDirectory(url)
    }
}
