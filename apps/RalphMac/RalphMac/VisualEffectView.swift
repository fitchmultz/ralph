/**
 VisualEffectView

 Responsibilities:
 - Provide an NSViewRepresentable wrapper for NSVisualEffectView to enable
   native macOS glass morphism effects in SwiftUI.
 - Support all NSVisualEffectView.Material types (.sidebar, .contentBackground, etc.)
 - Support blending modes (.behindWindow, .withinWindow) for different layering effects.

 Does not handle:
 - Animation or transitions (handled by SwiftUI)
 - Color tinting beyond vibrancy (use SwiftUI modifiers)

 Invariants/assumptions callers must respect:
 - Must be used within a macOS app (NSVisualEffectView is AppKit)
 - Effects render correctly in both light and dark mode when configured properly
 */

public import AppKit
public import SwiftUI

/// A SwiftUI view that wraps NSVisualEffectView for native glass morphism effects.
public struct VisualEffectView: NSViewRepresentable {
    /// The material type for the visual effect (e.g., .sidebar, .contentBackground)
    public var material: NSVisualEffectView.Material

    /// The blending mode determines how the effect composites with content
    public var blendingMode: NSVisualEffectView.BlendingMode

    /// Whether the effect is active (can be disabled for performance or testing)
    public var isEmphasized: Bool

    public init(
        material: NSVisualEffectView.Material = .contentBackground,
        blendingMode: NSVisualEffectView.BlendingMode = .behindWindow,
        isEmphasized: Bool = false
    ) {
        self.material = material
        self.blendingMode = blendingMode
        self.isEmphasized = isEmphasized
    }

    public func makeNSView(context: Context) -> NSVisualEffectView {
        let view = NSVisualEffectView()
        view.material = material
        view.blendingMode = blendingMode
        view.state = isEmphasized ? .active : .followsWindowActiveState
        view.wantsLayer = true
        return view
    }

    public func updateNSView(_ nsView: NSVisualEffectView, context: Context) {
        nsView.material = material
        nsView.blendingMode = blendingMode
        nsView.state = isEmphasized ? .active : .followsWindowActiveState
    }
}

// MARK: - SwiftUI Modifiers for Glass Morphism

public extension View {
    /// Applies a glass morphism background using the specified material
    func glassBackground(
        _ material: NSVisualEffectView.Material = .contentBackground,
        blendingMode: NSVisualEffectView.BlendingMode = .behindWindow,
        isEmphasized: Bool = false,
        cornerRadius: CGFloat = 0
    ) -> some View {
        self.background(
            VisualEffectView(
                material: material,
                blendingMode: blendingMode,
                isEmphasized: isEmphasized
            )
            .clipShape(RoundedRectangle(cornerRadius: cornerRadius, style: .continuous))
        )
    }

    /// Applies an under-page background for layered depth effects
    func underPageBackground(
        cornerRadius: CGFloat = 8,
        isEmphasized: Bool = false
    ) -> some View {
        self.background(
            VisualEffectView(
                material: .underPageBackground,
                blendingMode: .behindWindow,
                isEmphasized: isEmphasized
            )
            .clipShape(RoundedRectangle(cornerRadius: cornerRadius, style: .continuous))
        )
    }

    /// Applies a content background material
    func contentBackground(
        cornerRadius: CGFloat = 0,
        isEmphasized: Bool = false
    ) -> some View {
        self.background(
            VisualEffectView(
                material: .contentBackground,
                blendingMode: .withinWindow,
                isEmphasized: isEmphasized
            )
            .clipShape(RoundedRectangle(cornerRadius: cornerRadius, style: .continuous))
        )
    }

    /// Applies a sidebar material (ideal for navigation sidebars)
    func sidebarBackground(
        isEmphasized: Bool = false
    ) -> some View {
        self.background(
            VisualEffectView(
                material: .sidebar,
                blendingMode: .behindWindow,
                isEmphasized: isEmphasized
            )
        )
    }
}

// MARK: - Glass Button Style

/// Button style with glass morphism hover effect
public struct GlassButtonStyle: ButtonStyle {
    @State private var isHovered = false

    public init() {}

    public func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .padding(.horizontal, 12)
            .padding(.vertical, 6)
            .background(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(.quaternary.opacity(isHovered ? 0.5 : 0.2))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .strokeBorder(.separator.opacity(isHovered ? 0.5 : 0.3), lineWidth: 0.5)
            )
            .scaleEffect(configuration.isPressed ? 0.97 : 1.0)
            .animation(.easeInOut(duration: 0.15), value: isHovered)
            .animation(.easeInOut(duration: 0.1), value: configuration.isPressed)
            .onHover { hovering in
                isHovered = hovering
            }
    }
}
