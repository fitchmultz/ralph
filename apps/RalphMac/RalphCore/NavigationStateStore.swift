//!
//! NavigationStateStore
//!
//! Purpose:
//! - Persist workspace-local navigation state through an explicit, testable store.
//!
//! Responsibilities:
//! - Encode and decode `NavigationState` snapshots.
//! - Surface storage failures to callers instead of swallowing them.
//!
//! Scope:
//! - Navigation persistence only.
//!
//! Usage:
//! - `NavigationViewModel` should be the primary consumer.
//!
//! Invariants/Assumptions:
//! - Callers own version handling and failure presentation.
//! - Storage keys are provided by the caller and remain namespaced outside the store.

public import Foundation

public struct NavigationStateStore {
    public typealias LoadData = (String) throws -> Data?
    public typealias SaveData = (Data, String) throws -> Void
    public typealias RemoveData = (String) -> Void

    private let loadData: LoadData
    private let saveData: SaveData
    private let removeData: RemoveData

    public init(
        loadData: @escaping LoadData,
        saveData: @escaping SaveData,
        removeData: @escaping RemoveData
    ) {
        self.loadData = loadData
        self.saveData = saveData
        self.removeData = removeData
    }

    public init(defaults: UserDefaults = RalphAppDefaults.userDefaults) {
        self.init(
            loadData: { defaults.data(forKey: $0) },
            saveData: { data, key in defaults.set(data, forKey: key) },
            removeData: { defaults.removeObject(forKey: $0) }
        )
    }

    public func loadState(forKey key: String) throws -> NavigationState? {
        guard let data = try loadData(key) else {
            return nil
        }
        return try JSONDecoder().decode(NavigationState.self, from: data)
    }

    public func saveState(_ state: NavigationState, forKey key: String) throws {
        let data = try JSONEncoder().encode(state)
        try saveData(data, key)
    }

    public func removeState(forKey key: String) {
        removeData(key)
    }
}
