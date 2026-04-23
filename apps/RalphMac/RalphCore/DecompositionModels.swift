/**
 DecompositionModels

 Responsibilities:
 - Mirror the stable JSON payloads emitted by `ralph machine task decompose`.
 - Provide app-side request models for previewing and writing task decompositions.
 - Keep decoding logic centralized so views and workspace operations stay type-safe.

 Does not handle:
 - CLI process execution or retry behavior.
 - Local decomposition logic or queue mutation.

 Invariants/assumptions callers must respect:
 - JSON decoding must stay aligned with the Rust CLI contract.
 - Preview responses are wrapped in a versioned envelope with `preview` and optional `write` fields.
 - Existing-task decomposition and attach-to-freeform decomposition remain distinct user intents.
 */

import Foundation

public enum DecompositionChildPolicy: String, Decodable, Sendable, Equatable, CaseIterable, Identifiable {
    case fail
    case append
    case replace

    public var id: String { rawValue }

    public var displayName: String {
        switch self {
        case .fail: return "Fail"
        case .append: return "Append"
        case .replace: return "Replace"
        }
    }

    public var helpText: String {
        switch self {
        case .fail:
            return "Refuse writes when the effective parent already has children."
        case .append:
            return "Keep existing children and add the new subtree after them."
        case .replace:
            return "Replace the existing child subtree after CLI safety checks pass."
        }
    }
}

public enum TaskDecomposeSourceInput: Sendable, Equatable {
    case freeform(String)
    case existingTaskID(String)
}

public struct TaskDecomposeOptions: Sendable, Equatable {
    public var attachToTaskID: String?
    public var maxDepth: Int
    public var maxChildren: Int
    public var maxNodes: Int
    public var status: RalphTaskStatus
    public var childPolicy: DecompositionChildPolicy
    public var withDependencies: Bool

    public init(
        attachToTaskID: String? = nil,
        maxDepth: Int = 3,
        maxChildren: Int = 5,
        maxNodes: Int = 50,
        status: RalphTaskStatus = .draft,
        childPolicy: DecompositionChildPolicy = .fail,
        withDependencies: Bool = false
    ) {
        self.attachToTaskID = attachToTaskID
        self.maxDepth = maxDepth
        self.maxChildren = maxChildren
        self.maxNodes = maxNodes
        self.status = status
        self.childPolicy = childPolicy
        self.withDependencies = withDependencies
    }
}

public enum DecompositionSource: Sendable, Equatable {
    case freeform(request: String)
    case existingTask(task: RalphTask)
}

extension DecompositionSource: Decodable {
    private enum CodingKeys: String, CodingKey {
        case kind
        case request
        case task
    }

    private enum Kind: String, Decodable {
        case freeform
        case existingTask = "existing_task"
    }

    public init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        switch try container.decode(Kind.self, forKey: .kind) {
        case .freeform:
            self = .freeform(request: try container.decode(String.self, forKey: .request))
        case .existingTask:
            self = .existingTask(task: try container.decode(RalphTask.self, forKey: .task))
        }
    }
}

public struct DecompositionAttachTarget: Decodable, Sendable, Equatable {
    public let task: RalphTask
    public let hasExistingChildren: Bool

    private enum CodingKeys: String, CodingKey {
        case task
        case hasExistingChildren = "has_existing_children"
    }
}

public struct DependencyEdgePreview: Decodable, Sendable, Equatable, Identifiable {
    public let taskTitle: String
    public let dependsOnTitle: String

    public var id: String {
        "\(taskTitle)->\(dependsOnTitle)"
    }

    private enum CodingKeys: String, CodingKey {
        case taskTitle = "task_title"
        case dependsOnTitle = "depends_on_title"
    }
}

public struct PlannedNode: Decodable, Sendable, Equatable, Identifiable {
    public let plannerKey: String
    public let title: String
    public let description: String?
    public let plan: [String]
    public let tags: [String]
    public let scope: [String]
    public let dependsOnKeys: [String]
    public let children: [PlannedNode]

    public var id: String { plannerKey }

    public var isLeaf: Bool { children.isEmpty }

    private enum CodingKeys: String, CodingKey {
        case plannerKey = "planner_key"
        case title
        case description
        case plan
        case tags
        case scope
        case dependsOnKeys = "depends_on_keys"
        case children
    }
}

public struct DecompositionPlan: Decodable, Sendable, Equatable {
    public let root: PlannedNode
    public let warnings: [String]
    public let totalNodes: Int
    public let leafNodes: Int
    public let dependencyEdges: [DependencyEdgePreview]

    private enum CodingKeys: String, CodingKey {
        case root
        case warnings
        case totalNodes = "total_nodes"
        case leafNodes = "leaf_nodes"
        case dependencyEdges = "dependency_edges"
    }
}

public struct DecompositionPreview: Decodable, Sendable, Equatable {
    public let source: DecompositionSource
    public let attachTarget: DecompositionAttachTarget?
    public let plan: DecompositionPlan
    public let writeBlockers: [String]
    public let childStatus: RalphTaskStatus
    public let childPolicy: DecompositionChildPolicy
    public let withDependencies: Bool

    private enum CodingKeys: String, CodingKey {
        case source
        case attachTarget = "attach_target"
        case plan
        case writeBlockers = "write_blockers"
        case childStatus = "child_status"
        case childPolicy = "child_policy"
        case withDependencies = "with_dependencies"
    }
}

public struct TaskDecomposeWriteResult: Decodable, Sendable, Equatable {
    public let rootTaskID: String?
    public let parentTaskID: String?
    public let createdIDs: [String]
    public let replacedIDs: [String]
    public let parentAnnotated: Bool

    private enum CodingKeys: String, CodingKey {
        case rootTaskID = "root_task_id"
        case parentTaskID = "parent_task_id"
        case createdIDs = "created_ids"
        case replacedIDs = "replaced_ids"
        case parentAnnotated = "parent_annotated"
    }
}

public struct TaskDecomposeEnvelope: Decodable, Sendable, Equatable {
    public let version: Int
    public let mode: String
    public let preview: DecompositionPreview
    public let write: TaskDecomposeWriteResult?
}

struct MachineDecomposeDocument: Decodable, Sendable, Equatable, VersionedMachineDocument {
    static let expectedVersion = RalphMachineContract.taskDecomposeVersion
    static let documentName = "machine task decompose"

    let version: Int
    let blocking: WorkspaceRunnerController.MachineBlockingState?
    let result: TaskDecomposeEnvelope
    let continuation: WorkspaceContinuationSummary

    var effectiveBlocking: WorkspaceRunnerController.MachineBlockingState? {
        blocking ?? continuation.blocking
    }
}
