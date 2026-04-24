/**
 KanbanTransientState

 Purpose:
 - Own transient Kanban board UI state that should not live inline in the view.

 Responsibilities:
 - Own transient Kanban board UI state that should not live inline in the view.
 - Coordinate externally-triggered highlight presentation for recently changed tasks.

 Does not handle:
 - Column rendering, task selection, or status mutations.
 - Task persistence.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Only external queue refresh events should trigger highlight presentation.
 - Repeated refresh events replace any in-flight highlight clearing task.
 */

import Foundation
import RalphCore
import SwiftUI

@MainActor
final class KanbanTransientState: ObservableObject {
    @Published private(set) var highlightedTaskIDs: Set<String> = []

    private var highlightResetTask: Task<Void, Never>?

    deinit {
        highlightResetTask?.cancel()
    }

    func handleQueueRefreshEvent(_ refreshEvent: Workspace.QueueRefreshEvent?) {
        guard let refreshEvent, refreshEvent.source == .externalFileChange else {
            return
        }

        highlightResetTask?.cancel()

        withAnimation(.easeInOut(duration: 0.3)) {
            highlightedTaskIDs = refreshEvent.highlightedTaskIDs
        }

        highlightResetTask = Task { @MainActor [weak self] in
            guard let self else { return }
            do {
                try await Task.sleep(for: .seconds(2))
                withAnimation(.easeInOut(duration: 0.5)) {
                    self.highlightedTaskIDs.removeAll()
                }
            } catch {
                return
            }
        }
    }
}
