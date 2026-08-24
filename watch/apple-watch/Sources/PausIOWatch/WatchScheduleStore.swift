import Foundation

public enum WatchTimerPhase: Codable, Sendable, Equatable {
    case dormant
    case working
    case preBreak
    case breakDue(kind: String?)
    case breaking(kind: String?)
    case paused
    case unknown

    private struct Detail: Codable {
        let kind: String?
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        if let value = try? container.decode(String.self) {
            switch value {
            case "dormant": self = .dormant
            case "working": self = .working
            case "pre_break": self = .preBreak
            case "break_due": self = .breakDue(kind: nil)
            case "breaking": self = .breaking(kind: nil)
            case "paused": self = .paused
            default: self = .unknown
            }
            return
        }
        let value = try container.decode([String: Detail].self)
        if let detail = value["break_due"] {
            self = .breakDue(kind: detail.kind)
        } else if let detail = value["breaking"] {
            self = .breaking(kind: detail.kind)
        } else if value["paused"] != nil {
            self = .paused
        } else {
            self = .unknown
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case .dormant: try container.encode("dormant")
        case .working: try container.encode("working")
        case .preBreak: try container.encode("pre_break")
        case let .breakDue(kind): try container.encode(["break_due": Detail(kind: kind)])
        case let .breaking(kind): try container.encode(["breaking": Detail(kind: kind)])
        case .paused: try container.encode("paused")
        case .unknown: try container.encode("unknown")
        }
    }

    var suspendsSchedule: Bool {
        self == .dormant || self == .paused
    }

    var isBreakInProgress: Bool {
        if case .breaking = self { return true }
        return false
    }

    var breakKind: String? {
        switch self {
        case let .breakDue(kind), let .breaking(kind): return kind
        default: return nil
        }
    }

    func duration(in settings: WatchSettingsEnvelope) -> Int? {
        switch self {
        case .working: return settings.workIntervalSeconds
        case .preBreak: return settings.preBreakSeconds
        case .breaking:
            return (breakKind ?? settings.breakKind) == "long"
                ? settings.longBreakSeconds
                : settings.shortBreakSeconds
        case .dormant, .breakDue, .paused, .unknown: return nil
        }
    }
}

public struct WatchSettingsEnvelope: Codable, Sendable, Equatable {
    public let schemaVersion: Int
    public let revision: UInt64
    public let timezone: String
    public let workIntervalSeconds: Int
    public let shortBreakSeconds: Int
    public let longBreakSeconds: Int
    public let preBreakSeconds: Int
    public let activeDaysMask: Int
    public let activeStartMinutes: Int
    public let activeEndMinutes: Int
    public let paused: Bool
    public let updatedAt: Date
    public let nextBreakAt: Date?
    public let breakActive: Bool?
    public let breakKind: String?
    /// Added after the initial v1 payload. Older phones omit these fields.
    public let phase: WatchTimerPhase?
    public let phaseDeadlineAt: Date?

    enum CodingKeys: String, CodingKey { case schemaVersion = "schema_version", revision, timezone, workIntervalSeconds = "work_interval_seconds", shortBreakSeconds = "short_break_seconds", longBreakSeconds = "long_break_seconds", preBreakSeconds = "pre_break_seconds", activeDaysMask = "active_days_mask", activeStartMinutes = "active_start_minutes", activeEndMinutes = "active_end_minutes", paused, updatedAt = "updated_at", nextBreakAt = "next_break_at", breakActive = "break_active", breakKind = "break_kind", phase, phaseDeadlineAt = "phase_deadline_at" }

    public static func localDefaults(now: Date = Date(), timeZone: TimeZone = .current) -> Self {
        Self(
            schemaVersion: 1,
            revision: 0,
            timezone: timeZone.identifier,
            workIntervalSeconds: 1_200,
            shortBreakSeconds: 20,
            longBreakSeconds: 300,
            preBreakSeconds: 30,
            activeDaysMask: 0b0111_1111,
            activeStartMinutes: 0,
            activeEndMinutes: 0,
            paused: false,
            updatedAt: now,
            nextBreakAt: now.addingTimeInterval(1_200),
            breakActive: false,
            breakKind: nil,
            phase: nil,
            phaseDeadlineAt: nil
        )
    }

    /// This is intentionally identical to the Rust/Kotlin v1 validation gate.
    /// Optional phase fields remain additive and therefore do not change v1 validity.
    public var isValid: Bool {
        schemaVersion == 1 && TimeZone(identifier: timezone) != nil && workIntervalSeconds >= 300 && workIntervalSeconds <= 7_200 &&
            shortBreakSeconds >= 5 && shortBreakSeconds <= 120 && longBreakSeconds >= 5 && longBreakSeconds <= 3_600 &&
            [0, 10, 30, 60].contains(preBreakSeconds) && activeDaysMask != 0 && activeDaysMask <= 127 &&
            (0..<1_440).contains(activeStartMinutes) && (0..<1_440).contains(activeEndMinutes)
    }
}

public enum WatchRuntimeAction: String, Codable, Sendable, CaseIterable {
    case pause
    case resume
    case takeBreakNow = "take_break_now"
    case skipBreak = "skip_break"
}

/// Runtime commands are deliberately short-lived: an unreachable iPhone does
/// not receive a queued replay. A later settings revision is authoritative.
public struct WatchRuntimeActionV1: Codable, Sendable, Equatable {
    public let schemaVersion: Int
    public let actionID: UUID
    public let action: WatchRuntimeAction
    public let baseRevision: UInt64
    public let occurredAt: Date

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version", actionID = "action_id", action
        case baseRevision = "base_revision", occurredAt = "occurred_at"
    }

    public var isValid: Bool { schemaVersion == 1 }
}

public actor WatchScheduleStore {
    private(set) var current: WatchSettingsEnvelope?
    public init() {}

    /// The phone owns the schedule. Revisions are normally monotonic, but a
    /// phone reinstall starts its local counter again while the watch retains
    /// its last context. In that recovery case, a newer phone timestamp must
    /// be allowed to replace the stale cached revision.
    public static func shouldApply(
        _ candidate: WatchSettingsEnvelope,
        over current: WatchSettingsEnvelope?
    ) -> Bool {
        guard candidate.isValid else { return false }
        guard let current else { return true }
        // Timestamps order contexts across a phone reinstall. A revision only
        // breaks an equal-time tie, preserving the usual monotonic protocol.
        return candidate.updatedAt > current.updatedAt ||
            (candidate.updatedAt == current.updatedAt && candidate.revision > current.revision)
    }

    @discardableResult public func apply(_ value: WatchSettingsEnvelope) -> Bool {
        guard Self.shouldApply(value, over: current) else { return false }
        current = value
        return true
    }
    public func nextBreak(after date: Date) -> Date? {
        guard let current, !current.paused else { return nil }
        return date.addingTimeInterval(TimeInterval(current.workIntervalSeconds))
    }

    public static func remainingSeconds(startedAt: Date, durationSeconds: Int, now: Date) -> Int {
        max(0, min(durationSeconds, durationSeconds - Int(now.timeIntervalSince(startedAt))))
    }

    /// 1 is a new interval and 0 is a completed interval, matching the desktop dial.
    public static func remainingFraction(startedAt: Date, durationSeconds: Int, now: Date) -> Double {
        guard durationSeconds > 0 else { return 0 }
        let remaining = Double(remainingSeconds(
            startedAt: startedAt,
            durationSeconds: durationSeconds,
            now: now
        ))
        return min(1, max(0, remaining / Double(durationSeconds)))
    }

    /// Produces a bounded offline reminder plan from the synced local schedule.
    /// The plan intentionally contains only fire dates, never desktop activity
    /// or personal content. WatchOS allows at most 64 pending local
    /// notifications, so callers refresh this on activation and every settings
    /// sync rather than attempting an unbounded repeating timer.
    public static func reminderDates(
        for settings: WatchSettingsEnvelope,
        firstBreakAt: Date?,
        now: Date,
        limit: Int = 64
    ) -> [Date] {
        guard !settings.paused, settings.workIntervalSeconds > 0, limit > 0 else { return [] }
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: settings.timezone) ?? .current
        let interval = TimeInterval(settings.workIntervalSeconds)
        var candidate = max(firstBreakAt ?? now.addingTimeInterval(interval), now.addingTimeInterval(1))
        var reminders: [Date] = []

        // A cap prevents an invalid schedule from spinning forever while still
        // allowing a full 64-event plan across normal weekly working hours.
        for _ in 0..<(limit * 32) where reminders.count < limit {
            if isActive(settings, at: candidate, calendar: calendar) {
                reminders.append(candidate)
            }
            candidate = candidate.addingTimeInterval(interval)
        }
        return reminders
    }

    /// Mirrors the engine's current-day weekday rule for regular, overnight,
    /// and all-day schedules.
    public static func isActive(
        _ settings: WatchSettingsEnvelope,
        at date: Date,
        calendar: Calendar? = nil
    ) -> Bool {
        var scheduleCalendar = calendar ?? Calendar(identifier: .gregorian)
        if calendar == nil {
            scheduleCalendar.timeZone = TimeZone(identifier: settings.timezone) ?? .current
        }
        let components = scheduleCalendar.dateComponents([.weekday, .hour, .minute], from: date)
        let weekday = max(0, (components.weekday ?? 1) - 1)
        guard (settings.activeDaysMask & (1 << weekday)) != 0 else { return false }
        let minutes = (components.hour ?? 0) * 60 + (components.minute ?? 0)
        if settings.activeStartMinutes == settings.activeEndMinutes { return true }
        if settings.activeStartMinutes < settings.activeEndMinutes {
            return minutes >= settings.activeStartMinutes && minutes < settings.activeEndMinutes
        }
        return minutes >= settings.activeStartMinutes || minutes < settings.activeEndMinutes
    }
}
