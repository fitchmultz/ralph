/**
 TaskListTransientState

 Purpose:
 - Own transient UI state for `TaskListView` that should not live inline in the view body.

 Responsibilities:
 - Own transient UI state for `TaskListView` that should not live inline in the view body.
 - Coordinate externally-triggered queue refresh feedback, including highlighted rows and banner visibility.
 - Centralize cancellable refresh-feedback sequencing so repeated refresh events replace stale UI work.

 Does not handle:
 - Task filtering, sorting, or persistence.
 - Primary task selection rules or keyboard focus wiring.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/assumptions callers must respect:
 - Access is confined to the main actor because it drives SwiftUI view state.
 - Only external queue refresh events should trigger highlight/banner presentation.
 */

import Foundation
import RalphCore
import SwiftUI

@MainActor
final class TaskListTransientState: ObservableObject {
    @Published var showingBulkActions = false
    @Published private(set) var highlightedTaskIDs: Set<String> = []
    @Published private(set) var isExternalUpdateBannerVisible = false

    private var refreshFeedbackTask: Task<Void, Never>?

    deinit {
        refreshFeedbackTask?.cancel()
    }

    func handleQueueRefreshEvent(_ refreshEvent: Workspace.QueueRefreshEvent?) {
        guard let refreshEvent, refreshEvent.source == .externalFileChange else {
            return
        }

        refreshFeedbackTask?.cancel()
        highlightedTaskIDs = refreshEvent.highlightedTaskIDs

        withAnimation(.easeInOut(duration: 0.3)) {
            isExternalUpdateBannerVisible = true
        }

        withAnimation(.easeInOut(duration: 0.2)) {
            highlightedTaskIDs = refreshEvent.highlightedTaskIDs
        }

        refreshFeedbackTask = Task { @MainActor [weak self] in
            guard let self else { return }

            do {
                try await Task.sleep(for: .seconds(2))
                withAnimation(.easeInOut(duration: 0.5)) {
                    self.highlightedTaskIDs.removeAll()
                }

                try await Task.sleep(for: .seconds(1))
                withAnimation(.easeInOut(duration: 0.3)) {
                    self.isExternalUpdateBannerVisible = false
                }
            } catch {
                return
            }
        }
    }

    func clearSelection(
        selectedTaskIDs: Binding<Set<String>>,
        selectedTaskID: Binding<String?>
    ) {
        selectedTaskIDs.wrappedValue.removeAll()
        selectedTaskID.wrappedValue = nil
    }

    func resetForRepositoryRetarget() {
        refreshFeedbackTask?.cancel()
        refreshFeedbackTask = nil
        highlightedTaskIDs.removeAll()
        isExternalUpdateBannerVisible = false
        showingBulkActions = false
    }
}
