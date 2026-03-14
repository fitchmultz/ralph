/**
 GraphAlgorithmsTests

 Responsibilities:
 - Validate cycle detection algorithm correctness.
 - Test various graph topologies (acyclic, simple cycle, nested cycles, self-loops).
 - Verify wouldCreateCycle prevention logic.

 Does not handle:
 - UI integration (covered by UI tests).
 - Performance testing (covered by performance tests).

 Invariants/assumptions callers must respect:
 - Test graphs are representative of real dependency structures.
 - Cycle detection excludes relatesTo edges by design.
 */

import Foundation
import XCTest
@testable import RalphCore

final class GraphAlgorithmsTests: RalphCoreTestCase {

    // MARK: - detectCycles Tests

    func test_detectCycles_emptyGraph_returnsNoCycles() {
        let edges: [GraphEdge] = []

        let result = GraphAlgorithms.detectCycles(edges: edges)

        XCTAssertFalse(result.hasCycle)
        XCTAssertTrue(result.cycles.isEmpty)
    }

    func test_detectCycles_noCycles_returnsNoCycles() {
        // A → B → C (linear chain, no cycles)
        let edges = [
            GraphEdge(from: "A", to: "B", type: .dependency),
            GraphEdge(from: "B", to: "C", type: .dependency)
        ]

        let result = GraphAlgorithms.detectCycles(edges: edges)

        XCTAssertFalse(result.hasCycle)
        XCTAssertTrue(result.cycles.isEmpty)
    }

    func test_detectCycles_simpleCycle_detectsCycle() {
        // A → B → C → A (simple cycle)
        let edges = [
            GraphEdge(from: "A", to: "B", type: .dependency),
            GraphEdge(from: "B", to: "C", type: .dependency),
            GraphEdge(from: "C", to: "A", type: .dependency)
        ]

        let result = GraphAlgorithms.detectCycles(edges: edges)

        XCTAssertTrue(result.hasCycle)
        XCTAssertEqual(result.cycles.count, 1)
        // Cycle should be normalized (start from smallest element)
        XCTAssertEqual(result.cycles.first?.sorted(), ["A", "B", "C"])
    }

    func test_detectCycles_selfLoop_detectsCycle() {
        // A → A (self-loop)
        let edges = [
            GraphEdge(from: "A", to: "A", type: .dependency)
        ]

        let result = GraphAlgorithms.detectCycles(edges: edges)

        XCTAssertTrue(result.hasCycle)
        XCTAssertEqual(result.cycles.count, 1)
        XCTAssertEqual(result.cycles.first, ["A"])
    }

    func test_detectCycles_multipleCycles_detectsAll() {
        // A → B → C → A (cycle 1)
        // D → E → D (cycle 2, disconnected)
        let edges = [
            GraphEdge(from: "A", to: "B", type: .dependency),
            GraphEdge(from: "B", to: "C", type: .dependency),
            GraphEdge(from: "C", to: "A", type: .dependency),
            GraphEdge(from: "D", to: "E", type: .dependency),
            GraphEdge(from: "E", to: "D", type: .dependency)
        ]

        let result = GraphAlgorithms.detectCycles(edges: edges)

        XCTAssertTrue(result.hasCycle)
        XCTAssertEqual(result.cycles.count, 2)
    }

    func test_detectCycles_nestedCycles_detectsAll() {
        // A → B → C → A (outer cycle)
        // B → C → D → B (inner cycle sharing edges)
        let edges = [
            GraphEdge(from: "A", to: "B", type: .dependency),
            GraphEdge(from: "B", to: "C", type: .dependency),
            GraphEdge(from: "C", to: "A", type: .dependency),
            GraphEdge(from: "C", to: "D", type: .dependency),
            GraphEdge(from: "D", to: "B", type: .dependency)
        ]

        let result = GraphAlgorithms.detectCycles(edges: edges)

        XCTAssertTrue(result.hasCycle)
        // Should detect at least 2 distinct cycles
        XCTAssertGreaterThanOrEqual(result.cycles.count, 2)
    }

    func test_detectCycles_ignoresRelatesToEdges() {
        // relatesTo edges should NOT participate in cycle detection
        // (they are inherently bidirectional/symmetric)
        let edges = [
            GraphEdge(from: "A", to: "B", type: .relatesTo),
            GraphEdge(from: "B", to: "A", type: .relatesTo)
        ]

        let result = GraphAlgorithms.detectCycles(edges: edges)

        XCTAssertFalse(result.hasCycle)
    }

    func test_detectCycles_blocksRelationships_detectsCycles() {
        // blocks relationships can also form cycles (mutual blocking)
        let edges = [
            GraphEdge(from: "A", to: "B", type: .blocks),
            GraphEdge(from: "B", to: "A", type: .blocks)
        ]

        let result = GraphAlgorithms.detectCycles(edges: edges)

        XCTAssertTrue(result.hasCycle)
        XCTAssertEqual(result.cycles.count, 1)
    }

    func test_detectCycles_mixedEdgeTypes_detectsCombinedCycles() {
        // A -(depends_on)→ B -(blocks)→ C -(depends_on)→ A
        let edges = [
            GraphEdge(from: "A", to: "B", type: .dependency),
            GraphEdge(from: "B", to: "C", type: .blocks),
            GraphEdge(from: "C", to: "A", type: .dependency)
        ]

        let result = GraphAlgorithms.detectCycles(edges: edges)

        XCTAssertTrue(result.hasCycle)
        XCTAssertEqual(result.cycles.count, 1)
    }

    func test_detectCycles_diamondShape_noCycles() {
        //     A
        //    / \
        //   B   C
        //    \ /
        //     D
        // Diamond shape - no cycles
        let edges = [
            GraphEdge(from: "D", to: "B", type: .dependency),
            GraphEdge(from: "D", to: "C", type: .dependency),
            GraphEdge(from: "B", to: "A", type: .dependency),
            GraphEdge(from: "C", to: "A", type: .dependency)
        ]

        let result = GraphAlgorithms.detectCycles(edges: edges)

        XCTAssertFalse(result.hasCycle)
        XCTAssertTrue(result.cycles.isEmpty)
    }

    func test_detectCycles_disconnectedAcyclicGraph_noCycles() {
        // A → B and C → D (disconnected, both acyclic)
        let edges = [
            GraphEdge(from: "A", to: "B", type: .dependency),
            GraphEdge(from: "C", to: "D", type: .dependency)
        ]

        let result = GraphAlgorithms.detectCycles(edges: edges)

        XCTAssertFalse(result.hasCycle)
    }

    // MARK: - wouldCreateCycle Tests

    func test_wouldCreateCycle_emptyGraph_returnsFalse() {
        let existing: [GraphEdge] = []
        let newEdge = GraphEdge(from: "A", to: "B", type: .dependency)

        let result = GraphAlgorithms.wouldCreateCycle(
            existingEdges: existing,
            newEdge: newEdge,
            allTaskIDs: ["A", "B"]
        )

        XCTAssertFalse(result)
    }

    func test_wouldCreateCycle_noCycle_returnsFalse() {
        // A → B exists, adding B → C should not create cycle
        let existing = [
            GraphEdge(from: "A", to: "B", type: .dependency)
        ]
        let newEdge = GraphEdge(from: "B", to: "C", type: .dependency)

        let result = GraphAlgorithms.wouldCreateCycle(
            existingEdges: existing,
            newEdge: newEdge,
            allTaskIDs: ["A", "B", "C"]
        )

        XCTAssertFalse(result)
    }

    func test_wouldCreateCycle_directBackEdge_returnsTrue() {
        // A → B exists, adding B → A would create cycle
        let existing = [
            GraphEdge(from: "A", to: "B", type: .dependency)
        ]
        let newEdge = GraphEdge(from: "B", to: "A", type: .dependency)

        let result = GraphAlgorithms.wouldCreateCycle(
            existingEdges: existing,
            newEdge: newEdge,
            allTaskIDs: ["A", "B"]
        )

        XCTAssertTrue(result)
    }

    func test_wouldCreateCycle_indirectBackEdge_returnsTrue() {
        // A → B → C exists, adding C → A would create cycle
        let existing = [
            GraphEdge(from: "A", to: "B", type: .dependency),
            GraphEdge(from: "B", to: "C", type: .dependency)
        ]
        let newEdge = GraphEdge(from: "C", to: "A", type: .dependency)

        let result = GraphAlgorithms.wouldCreateCycle(
            existingEdges: existing,
            newEdge: newEdge,
            allTaskIDs: ["A", "B", "C"]
        )

        XCTAssertTrue(result)
    }

    func test_wouldCreateCycle_selfLoop_returnsTrue() {
        // Adding A → A (self-loop) is always a cycle
        let existing: [GraphEdge] = []
        let newEdge = GraphEdge(from: "A", to: "A", type: .dependency)

        let result = GraphAlgorithms.wouldCreateCycle(
            existingEdges: existing,
            newEdge: newEdge,
            allTaskIDs: ["A"]
        )

        XCTAssertTrue(result)
    }

    func test_wouldCreateCycle_relatesToEdge_returnsFalse() {
        // relatesTo edges don't participate in cycle detection
        let existing = [
            GraphEdge(from: "A", to: "B", type: .dependency)
        ]
        let newEdge = GraphEdge(from: "B", to: "A", type: .relatesTo)

        let result = GraphAlgorithms.wouldCreateCycle(
            existingEdges: existing,
            newEdge: newEdge,
            allTaskIDs: ["A", "B"]
        )

        XCTAssertFalse(result)
    }

    func test_wouldCreateCycle_blocksCycle_returnsTrue() {
        // A -(blocks)→ B exists, adding B -(blocks)→ A would create cycle
        let existing = [
            GraphEdge(from: "A", to: "B", type: .blocks)
        ]
        let newEdge = GraphEdge(from: "B", to: "A", type: .blocks)

        let result = GraphAlgorithms.wouldCreateCycle(
            existingEdges: existing,
            newEdge: newEdge,
            allTaskIDs: ["A", "B"]
        )

        XCTAssertTrue(result)
    }

    // MARK: - edgesInCycles Tests

    func test_edgesInCycles_noCycles_returnsEmptySet() {
        let edges = [
            GraphEdge(from: "A", to: "B", type: .dependency),
            GraphEdge(from: "B", to: "C", type: .dependency)
        ]

        let cycleEdgeIDs = GraphAlgorithms.edgesInCycles(edges: edges)

        XCTAssertTrue(cycleEdgeIDs.isEmpty)
    }

    func test_edgesInCycles_returnsOnlyCycleEdges() {
        // A → B → C → A (cycle), plus C → D (acyclic branch)
        let cycleEdge1 = GraphEdge(from: "A", to: "B", type: .dependency)
        let cycleEdge2 = GraphEdge(from: "B", to: "C", type: .dependency)
        let cycleEdge3 = GraphEdge(from: "C", to: "A", type: .dependency)
        let acyclicEdge = GraphEdge(from: "C", to: "D", type: .dependency)

        let edges = [cycleEdge1, cycleEdge2, cycleEdge3, acyclicEdge]

        let cycleEdgeIDs = GraphAlgorithms.edgesInCycles(edges: edges)

        // Should contain A→B, B→C, C→A but NOT C→D
        XCTAssertEqual(cycleEdgeIDs.count, 3)
        XCTAssertTrue(cycleEdgeIDs.contains(cycleEdge1.id))
        XCTAssertTrue(cycleEdgeIDs.contains(cycleEdge2.id))
        XCTAssertTrue(cycleEdgeIDs.contains(cycleEdge3.id))
        XCTAssertFalse(cycleEdgeIDs.contains(acyclicEdge.id))
    }

    func test_edgesInCycles_selfLoop_returnsEdgeID() {
        let selfLoopEdge = GraphEdge(from: "A", to: "A", type: .dependency)
        let edges = [selfLoopEdge]

        let cycleEdgeIDs = GraphAlgorithms.edgesInCycles(edges: edges)

        XCTAssertEqual(cycleEdgeIDs.count, 1)
        XCTAssertTrue(cycleEdgeIDs.contains(selfLoopEdge.id))
    }

    func test_edgesInCycles_multipleCycles_returnsAllCycleEdges() {
        // A → B → A (cycle 1) and C → D → C (cycle 2)
        let edge1 = GraphEdge(from: "A", to: "B", type: .dependency)
        let edge2 = GraphEdge(from: "B", to: "A", type: .dependency)
        let edge3 = GraphEdge(from: "C", to: "D", type: .dependency)
        let edge4 = GraphEdge(from: "D", to: "C", type: .dependency)

        let edges = [edge1, edge2, edge3, edge4]

        let cycleEdgeIDs = GraphAlgorithms.edgesInCycles(edges: edges)

        XCTAssertEqual(cycleEdgeIDs.count, 4)
        XCTAssertTrue(cycleEdgeIDs.contains(edge1.id))
        XCTAssertTrue(cycleEdgeIDs.contains(edge2.id))
        XCTAssertTrue(cycleEdgeIDs.contains(edge3.id))
        XCTAssertTrue(cycleEdgeIDs.contains(edge4.id))
    }

    func test_edgesInCycles_ignoresRelatesToEdges() {
        // Cycle with mixed edge types: A -(depends)→ B -(relates)→ C -(depends)→ A
        // relatesTo edges should not be included in the result
        let depEdge1 = GraphEdge(from: "A", to: "B", type: .dependency)
        let relatesEdge = GraphEdge(from: "B", to: "C", type: .relatesTo)
        let depEdge2 = GraphEdge(from: "C", to: "A", type: .dependency)

        let edges = [depEdge1, relatesEdge, depEdge2]

        let cycleEdgeIDs = GraphAlgorithms.edgesInCycles(edges: edges)

        // Only dependency edges should be identified as in-cycle
        // (Note: the cycle detection itself also ignores relatesTo, so this is a partial cycle)
        XCTAssertFalse(cycleEdgeIDs.contains(relatesEdge.id))
    }

    // MARK: - CycleDetectionResult Tests

    func test_cycleDetectionResult_noCycles_constant() {
        let result = CycleDetectionResult.noCycles

        XCTAssertFalse(result.hasCycle)
        XCTAssertTrue(result.cycles.isEmpty)
    }

    func test_cycleDetectionResult_equality() {
        let result1 = CycleDetectionResult(hasCycle: true, cycles: [["A", "B", "C"]])
        let result2 = CycleDetectionResult(hasCycle: true, cycles: [["A", "B", "C"]])
        let result3 = CycleDetectionResult(hasCycle: true, cycles: [["A", "B"]])
        let result4 = CycleDetectionResult(hasCycle: false, cycles: [])

        XCTAssertEqual(result1, result2)
        XCTAssertNotEqual(result1, result3)
        XCTAssertNotEqual(result1, result4)
    }
}
