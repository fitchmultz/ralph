/**
 MachineContract

 Purpose:
 - Centralize RalphMac validation for versioned machine JSON documents emitted by the CLI.

 Responsibilities:
 - Define the machine document versions this app revision understands.
 - Fail fast when decoded CLI payloads use unsupported versions.
 - Provide shared decode helpers for direct machine JSON responses.

 Scope:
 - Machine contract validation only; CLI execution, queue mutation, and UI rendering live elsewhere.

 Usage:
 - Call `requireVersion(_:expected:document:operation:)` before consuming nested machine payloads.
 - Use `decode(_:from:operation:)` for top-level versioned machine documents.

 Invariants/Assumptions:
 - Every versioned machine document has a top-level `version` field.
 - Unsupported versions are surfaced as recovery errors instead of being treated as warnings.
 */

import Foundation

enum RalphMachineContract {
  public static let runEventVersion = 3
  public static let runSummaryVersion = 2
  public static let configResolveVersion = 3
  public static let workspaceOverviewVersion = 1
  public static let queueValidateVersion = 1
  public static let queueRepairVersion = 1
  public static let queueUndoVersion = 1
  public static let taskBuildVersion = 1
  public static let taskMutateVersion = 2
  public static let taskDecomposeVersion = 2
  public static let graphReadVersion = 1
  public static let dashboardReadVersion = 1
  public static let doctorReportVersion = 2
  public static let parallelStatusVersion = 3
  public static let queueReadVersion = 1
  public static let taskCreateVersion = 1
  public static let cliSpecVersion = 2
  public static let errorVersion = 1
  public static let queueUnlockInspectVersion = 1

  public static func requireVersion(
    _ actual: Int,
    expected: Int,
    document: String,
    operation: String
  ) throws {
    guard actual == expected else {
      throw RecoveryError(
        category: .versionMismatch,
        message:
          "Unsupported \(document) version \(actual). RalphMac requires version \(expected).",
        operation: operation,
        suggestions: ["Rebuild RalphMac and the bundled CLI from the same revision."]
      )
    }
  }

  public static func decode<T: Decodable & VersionedMachineDocument>(
    _ type: T.Type,
    from data: Data,
    operation: String
  ) throws -> T {
    let document = try JSONDecoder().decode(T.self, from: data)
    try requireVersion(
      document.version, expected: T.expectedVersion, document: T.documentName, operation: operation)
    return document
  }
}

protocol VersionedMachineDocument: Decodable {
  static var expectedVersion: Int { get }
  static var documentName: String { get }
  var version: Int { get }
}
