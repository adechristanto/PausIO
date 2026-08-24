import SwiftUI
import WidgetKit

private let sharedSuite = "group.com.pausio.app"

private struct PausIOComplicationEntry: TimelineEntry {
    let date: Date
    let title: String
    let deadline: Date?
    let durationSeconds: Int
}

private struct PausIOComplicationProvider: TimelineProvider {
    func placeholder(in context: Context) -> PausIOComplicationEntry {
        PausIOComplicationEntry(
            date: .now,
            title: "Focus",
            deadline: .now.addingTimeInterval(1200),
            durationSeconds: 1200
        )
    }

    func getSnapshot(in context: Context, completion: @escaping (PausIOComplicationEntry) -> Void) {
        completion(entry())
    }

    func getTimeline(in context: Context, completion: @escaping (Timeline<PausIOComplicationEntry>) -> Void) {
        let value = entry()
        completion(Timeline(entries: [value], policy: .after(.now.addingTimeInterval(15 * 60))))
    }

    private func entry() -> PausIOComplicationEntry {
        let store = UserDefaults(suiteName: sharedSuite)
        let title = store?.string(forKey: "pausio.complication.title") ?? "PausIO"
        let deadline = store?.object(forKey: "pausio.complication.deadline") as? Date
        let durationSeconds = max(
            1,
            store?.object(forKey: "pausio.complication.duration") as? Int ?? 1200
        )
        return PausIOComplicationEntry(
            date: .now,
            title: title,
            deadline: deadline,
            durationSeconds: durationSeconds
        )
    }
}

@main struct PausIOWatchComplication: Widget {
    var body: some WidgetConfiguration {
        StaticConfiguration(kind: "PausIOWatchComplication", provider: PausIOComplicationProvider()) { entry in
            PausIOComplicationView(entry: entry)
        }
        .configurationDisplayName("PausIO")
        .description("Your next eye-rest reminder.")
        .supportedFamilies([.accessoryCircular, .accessoryRectangular])
    }
}

private struct PausIOComplicationView: View {
    @Environment(\.widgetFamily) private var family
    let entry: PausIOComplicationEntry

    var body: some View {
        switch family {
        case .accessoryCircular:
            Gauge(value: progress) {
                Image(systemName: "eye")
            } currentValueLabel: {
                Text(entry.deadline ?? .now, style: .timer).monospacedDigit()
            }
            .widgetURL(URL(string: "pausio://watch"))
        default:
            HStack {
                Image(systemName: "eye")
                VStack(alignment: .leading) {
                    Text(entry.title).lineLimit(1)
                    if let deadline = entry.deadline { Text(deadline, style: .timer).monospacedDigit() }
                }
            }
            .widgetURL(URL(string: "pausio://watch"))
        }
    }

    private var progress: Double {
        guard let deadline = entry.deadline else { return 0 }
        return max(0, min(1, deadline.timeIntervalSinceNow / Double(entry.durationSeconds)))
    }
}
