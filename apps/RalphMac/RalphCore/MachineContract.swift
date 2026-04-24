//
//  MachineContract.swift
//  RalphMac
//
//  Purpose:
//  - Centralize machine-document version gates and shared decode entry points.
//
//  Responsibilities:
//  - Fail fast when RalphMac sees unsupported machine document versions.
//  - Provide reusable decoding helpers for versioned machine payloads.
//  - Keep contract validation out of feature-specific loading code.
//
//  Scope:
//  - Machine documents only; not CLI execution, queue mutation, or UI rendering.
//
//  Usage:
//  - Call `requireVersion(_:expected:document:operation:)` before consuming a
//    versioned machine document.
//  - Use the typed `decode...` helpers for direct CLI JSON payloads.
//
//  Invariants/Assumptions:
//  - Versioned machine documents always include a top-level `version` field.
//  - Unsupported versions are treated as contract failures, not soft warnings.
//

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
