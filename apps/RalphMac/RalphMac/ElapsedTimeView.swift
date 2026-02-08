/**
 ElapsedTimeView

 Responsibilities:
 - Display elapsed time since a given start date
 - Update every second while running
 - Format time in human-readable format (MM:SS or HH:MM:SS)

 Does not handle:
 - Time tracking logic (uses provided start time)
 - Timer management beyond display updates

 Invariants/assumptions callers must respect:
 - startTime is a valid Date in the past
 - View updates on main thread
 */

import SwiftUI

struct ElapsedTimeView: View {
    let startTime: Date
    @State private var currentTime = Date()
    @State private var timer: Timer?

    var body: some View {
        Text(formattedElapsedTime)
            .onAppear {
                startTimer()
            }
            .onDisappear {
                stopTimer()
            }
    }

    private var formattedElapsedTime: String {
        let elapsed = currentTime.timeIntervalSince(startTime)
        return formatDuration(elapsed)
    }

    private func formatDuration(_ duration: TimeInterval) -> String {
        let hours = Int(duration) / 3600
        let minutes = (Int(duration) % 3600) / 60
        let seconds = Int(duration) % 60

        if hours > 0 {
            return String(format: "%d:%02d:%02d", hours, minutes, seconds)
        } else {
            return String(format: "%d:%02d", minutes, seconds)
        }
    }

    private func startTimer() {
        // Update immediately
        currentTime = Date()

        // Then update every second
        timer = Timer.scheduledTimer(withTimeInterval: 1.0, repeats: true) { _ in
            currentTime = Date()
        }
    }

    private func stopTimer() {
        timer?.invalidate()
        timer = nil
    }
}

#Preview {
    VStack(spacing: 20) {
        ElapsedTimeView(startTime: Date().addingTimeInterval(-45))
        ElapsedTimeView(startTime: Date().addingTimeInterval(-3665))  // Over 1 hour
    }
    .padding()
}
