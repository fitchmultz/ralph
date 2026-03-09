/**
 RalphMacApp+Support

 Responsibilities:
 - Provide app-level support actions such as log export, crash-report export, and alerts.

 Does not handle:
 - URL routing.
 - Window/bootstrap lifecycle.

 Invariants/assumptions callers must respect:
 - AppKit save panels and alerts must run on the main actor.
 */

import AppKit
import Foundation
import RalphCore
import UniformTypeIdentifiers

extension RalphMacApp {
    func exportLogs() {
        guard RalphLogger.shared.canExportLogs else {
            showAlert(title: "Not Available", message: "Log export requires macOS 12 or later.")
            return
        }

        RalphLogger.shared.exportLogs(hours: 24) { logContent in
            guard let logContent else {
                Task { @MainActor in
                    showAlert(title: "Export Failed", message: "Could not retrieve logs.")
                }
                return
            }

            Task { @MainActor in
                let savePanel = NSSavePanel()
                savePanel.nameFieldStringValue = "ralph-logs-\(Date().formatted(.iso8601.dateSeparator(.dash).timeSeparator(.omitted))).txt"
                savePanel.allowedContentTypes = [.plainText]

                let result = await savePanel.begin()
                if result == .OK, let url = savePanel.url {
                    try? logContent.write(to: url, atomically: true, encoding: .utf8)
                }
            }
        }
    }

    func showCrashReports() {
        let reports = CrashReporter.shared.getAllReports()
        if reports.isEmpty {
            showAlert(title: "No Crash Reports", message: "No crash reports found.")
            return
        }

        let content = CrashReporter.shared.exportAllReports()

        Task { @MainActor in
            let savePanel = NSSavePanel()
            savePanel.nameFieldStringValue = "ralph-crash-reports-\(Date().formatted(.iso8601.dateSeparator(.dash))).txt"
            savePanel.allowedContentTypes = [.plainText]

            let result = await savePanel.begin()
            if result == .OK, let url = savePanel.url {
                do {
                    try content.write(to: url, atomically: true, encoding: .utf8)
                } catch {
                    showAlert(title: "Export Failed", message: "Could not save crash reports: \(error.localizedDescription)")
                }
            }
        }
    }

    func showAlert(title: String, message: String) {
        let alert = NSAlert()
        alert.messageText = title
        alert.informativeText = message
        alert.alertStyle = .informational
        alert.runModal()
    }
}
