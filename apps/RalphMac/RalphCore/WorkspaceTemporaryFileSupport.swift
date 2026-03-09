//!
//! WorkspaceTemporaryFileSupport
//!
//! Purpose:
//! - Centralize temporary file creation and cleanup for workspace CLI bridge flows.
//!
//! Responsibilities:
//! - Encode temporary JSON payloads to deterministic temp files.
//! - Surface cleanup/write failures through workspace persistence/operational health.
//! - Keep temp-file lifecycle code out of task-creation and task-mutation flows.
//!
//! Scope:
//! - Workspace-scoped temp files only.
//!
//! Usage:
//! - Call `withTemporaryJSONFile` around CLI commands that require an input file.
//!
//! Invariants/Assumptions:
//! - Temp-file write failures are fatal to the operation.
//! - Temp-file cleanup failures are surfaced but do not replace the primary operation result.

import Foundation

extension Workspace {
    func withTemporaryJSONFile<Payload: Encodable, Result>(
        prefix: String,
        payload: Payload,
        operationName: String,
        _ body: (URL) async throws -> Result
    ) async throws -> Result {
        let tempFileURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("\(prefix)-\(UUID().uuidString)", isDirectory: false)
            .appendingPathExtension("json")
        let issueContext = "\(operationName): \(tempFileURL.path)"

        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]

        do {
            try encoder.encode(payload).write(to: tempFileURL, options: .atomic)
        } catch {
            let issue = PersistenceIssue(
                domain: .temporaryFiles,
                operation: .save,
                context: issueContext,
                error: error
            )
            recordPersistenceIssue(issue)
            throw error
        }

        defer {
            do {
                try FileManager.default.removeItem(at: tempFileURL)
                clearPersistenceIssue(domain: .temporaryFiles, matchingContext: issueContext)
            } catch {
                recordPersistenceIssue(
                    PersistenceIssue(
                        domain: .temporaryFiles,
                        operation: .delete,
                        context: issueContext,
                        error: error
                    )
                )
            }
        }

        return try await body(tempFileURL)
    }
}
