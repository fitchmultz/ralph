/**
 AppSettings+Appearance

 Purpose:
 - Render appearance selection and preview controls for Settings.

 Responsibilities:
 - Render appearance selection and preview controls for Settings.
 - Bridge the shared app-appearance controller into the settings tab.

 Does not handle:
 - Window-level appearance application logic.
 - Runner or notification settings.

 Usage:
 - Used by the RalphMac app or RalphCore tests through its owning feature surface.

 Invariants/Assumptions:
 - Callers keep usage within the documented responsibilities and owning feature contracts.
 */

import SwiftUI

struct AppearanceSettingsTab: View {
    @ObservedObject private var appearance = AppAppearanceController.shared

    var body: some View {
        Form {
            Section("Theme") {
                Picker("Appearance:", selection: $appearance.selection) {
                    ForEach(AppColorScheme.allCases) { scheme in
                        Text(scheme.displayName).tag(scheme)
                    }
                }
                .pickerStyle(.radioGroup)

                Divider()

                VStack(alignment: .leading, spacing: 10) {
                    Text("Preview")
                        .font(.caption)
                        .foregroundStyle(.secondary)

                    RoundedRectangle(cornerRadius: 14)
                        .fill(previewBackground)
                        .frame(height: 140)
                        .overlay(alignment: .leading) {
                            VStack(alignment: .leading, spacing: 10) {
                                Text(previewTitle)
                                    .font(.headline)
                                    .foregroundStyle(previewForeground)

                                Text(appearance.selection.helperText)
                                    .foregroundStyle(previewForeground.opacity(0.8))

                                HStack(spacing: 8) {
                                    Capsule()
                                        .fill(Color.accentColor)
                                        .frame(width: 68, height: 24)
                                    Capsule()
                                        .fill(previewForeground.opacity(0.18))
                                        .frame(width: 96, height: 24)
                                }
                            }
                            .padding(18)
                        }
                }
            }

            Section {
                Text("Appearance changes apply immediately and persist across relaunch.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .padding()
    }

    private var previewTitle: String {
        switch appearance.selection {
        case .system:
            return "Match macOS"
        case .light:
            return "Light Appearance"
        case .dark:
            return "Dark Appearance"
        }
    }

    private var previewBackground: some ShapeStyle {
        switch appearance.selection {
        case .system:
            return AnyShapeStyle(
                LinearGradient(
                    colors: [Color(nsColor: .windowBackgroundColor), Color.accentColor.opacity(0.18)],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                )
            )
        case .light:
            return AnyShapeStyle(
                LinearGradient(
                    colors: [Color.white, Color(red: 0.9, green: 0.95, blue: 1.0)],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                )
            )
        case .dark:
            return AnyShapeStyle(
                LinearGradient(
                    colors: [Color(red: 0.12, green: 0.13, blue: 0.16), Color(red: 0.2, green: 0.24, blue: 0.31)],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                )
            )
        }
    }

    private var previewForeground: Color {
        switch appearance.selection {
        case .system:
            return .primary
        case .light:
            return Color(red: 0.14, green: 0.16, blue: 0.2)
        case .dark:
            return Color.white
        }
    }
}
