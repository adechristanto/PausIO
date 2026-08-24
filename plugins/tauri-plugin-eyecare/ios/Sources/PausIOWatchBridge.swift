import Foundation
import Tauri
import WatchConnectivity

enum WatchPhase: Codable {
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

    init(from decoder: Decoder) throws {
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

    func encode(to encoder: Encoder) throws {
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
}

struct WatchEnvelope: Codable {
    let schemaVersion: UInt16
    let revision: UInt64
    let timezone: String
    let workIntervalSeconds: UInt32
    let shortBreakSeconds: UInt32
    let longBreakSeconds: UInt32
    let preBreakSeconds: UInt32
    let activeDaysMask: UInt8
    let activeStartMinutes: UInt16
    let activeEndMinutes: UInt16
    let paused: Bool
    let updatedAt: String
    let nextBreakAt: String?
    let breakActive: Bool?
    let breakKind: String?
    /// These optional fields are sent by newer hosts and omitted by v1 hosts.
    let phase: WatchPhase?
    let phaseDeadlineAt: String?

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version", revision, timezone
        case workIntervalSeconds = "work_interval_seconds"
        case shortBreakSeconds = "short_break_seconds"
        case longBreakSeconds = "long_break_seconds"
        case preBreakSeconds = "pre_break_seconds"
        case activeDaysMask = "active_days_mask"
        case activeStartMinutes = "active_start_minutes"
        case activeEndMinutes = "active_end_minutes"
        case paused
        case updatedAt = "updated_at"
        case nextBreakAt = "next_break_at"
        case breakActive = "break_active"
        case breakKind = "break_kind"
        case phase
        case phaseDeadlineAt = "phase_deadline_at"
    }
}

struct BridgeStatus: Codable {
    let platform: String
    let available: Bool
    let paired: Bool
    let appInstalled: Bool
    let reachable: Bool
    let lastSyncedRevision: UInt64?
    let lastError: String?
    let lastQueuedRevision: UInt64?
    let connectionState: String
    let notificationPermission: String?
    let reminderPrecision: String?
    let scheduleHorizonAt: String?
    let lastSuccessfulSyncAt: String?
    let appVersion: String?
    let capabilities: BridgeCapabilities

    enum CodingKeys: String, CodingKey {
        case platform, available, paired
        case appInstalled = "app_installed"
        case reachable
        case lastSyncedRevision = "last_synced_revision"
        case lastError = "last_error"
        case lastQueuedRevision = "last_queued_revision"
        case connectionState = "connection_state"
        case notificationPermission = "notification_permission"
        case reminderPrecision = "reminder_precision"
        case scheduleHorizonAt = "schedule_horizon_at"
        case lastSuccessfulSyncAt = "last_successful_sync_at"
        case appVersion = "app_version"
        case capabilities
    }
}

struct BridgeCapabilities: Codable {
    let timerDisplay: Bool
    let localReminders: Bool
    let testHaptic: Bool
    let remoteActions: Bool
    let standalone: Bool
    let complication: Bool

    enum CodingKeys: String, CodingKey {
        case timerDisplay = "timer_display"
        case localReminders = "local_reminders"
        case testHaptic = "test_haptic"
        case remoteActions = "remote_actions"
        case standalone
        case complication
    }
}

private struct WatchRuntimeActionMessage: Codable {
    let schemaVersion: Int
    let actionID: String
    let action: String
    let baseRevision: UInt64
    let occurredAt: String

    enum CodingKeys: String, CodingKey {
        case schemaVersion = "schema_version", actionID = "action_id", action
        case baseRevision = "base_revision", occurredAt = "occurred_at"
    }

    var isValid: Bool {
        schemaVersion == 1 && !actionID.isEmpty &&
            ["pause", "resume", "take_break_now", "skip_break"].contains(action) &&
            ISO8601DateFormatter().date(from: occurredAt) != nil
    }
}

/// The iOS half of the Tauri mobile plugin. It transports settings, never timer ticks.
final class PausIOWatchBridge: NSObject, WCSessionDelegate {
    static let shared = PausIOWatchBridge()
    private override init() { super.init() }
    private let actionLock = NSLock()
    private var pendingActions: [String] = []
    private let supportedActions = ["pause", "resume", "take_break_now", "skip_break"]
    private var lastError: String?
    private let pendingContextKey = "pausio.pending_watch_context"
    private let queuedRevisionKey = "pausio.last_queued_revision"
    private let seenActionIDsKey = "pausio.seen_watch_runtime_action_ids"
    private let healthKey = "pausio.watch_health"

    func activate() {
        guard WCSession.isSupported() else {
            setLastError("WatchConnectivity is unavailable on this device")
            return
        }
        WCSession.default.delegate = self
        WCSession.default.activate()
    }

    func sync(_ envelope: WatchEnvelope) throws -> String {
        activate()
        guard WCSession.isSupported() else { return "unavailable" }
        let data = try JSONEncoder().encode(envelope)
        let context = try JSONSerialization.jsonObject(with: data) as? [String: Any] ?? [:]
        UserDefaults.standard.set(context, forKey: pendingContextKey)
        UserDefaults.standard.set(envelope.revision, forKey: queuedRevisionKey)
        guard WCSession.default.activationState == .activated else { return "queued" }
        return publishPendingContext()
    }

    @discardableResult private func publishPendingContext() -> String {
        guard let context = UserDefaults.standard.dictionary(forKey: pendingContextKey) else { return "queued" }
        guard WCSession.default.activationState == .activated else { return "queued" }
        do {
            let session = WCSession.default
            try session.updateApplicationContext(context)
            // Application context is the durable, latest-state channel. When the
            // paired app is active, also send the exact same envelope over the
            // immediate channel so phone controls do not wait for a background
            // delivery opportunity. The watch treats revisions idempotently.
            if session.isReachable {
                session.sendMessage(["kind": "settings", "envelope": context]) { reply in
                    if reply["accepted"] as? Bool == true {
                        self.setLastError(nil)
                    }
                } errorHandler: { error in
                    // The application context above remains queued, so an
                    // immediate-delivery failure is informative rather than data loss.
                    self.setLastError(error.localizedDescription)
                }
            }
            // Application context confirms hand-off to WatchConnectivity. The
            // watch advances this revision only after it applies the envelope.
            return "queued"
        } catch {
            setLastError(error.localizedDescription)
            return "unavailable"
        }
    }

    func sendTestNudge(_ completion: @escaping (String) -> Void) {
        activate()
        let session = WCSession.default
        guard session.isPaired, session.isWatchAppInstalled, session.isReachable else {
            setLastError("Watch is not currently reachable")
            completion("unavailable")
            return
        }
        let eventID = UUID().uuidString
        session.sendMessage(["kind": "test", "event_id": eventID]) { response in
            let acknowledgedID = response["event_id"] as? String
            guard response["accepted"] as? Bool == true, acknowledgedID == eventID else {
                self.setLastError("Watch rejected the test event")
                completion("unavailable")
                return
            }
            if response["notification_scheduled"] as? Bool == false {
                self.setLastError("Watch received the test event but could not schedule its notification")
            } else {
                self.setLastError(nil)
            }
            // This confirms event receipt only, never a physical haptic.
            completion("delivered")
        } errorHandler: { error in
            self.setLastError(error.localizedDescription)
            completion("unavailable")
        }
    }

    func takePendingAction() -> String? {
        actionLock.lock(); defer { actionLock.unlock() }
        guard !pendingActions.isEmpty else { return nil }
        return pendingActions.removeFirst()
    }

    func session(_ session: WCSession, activationDidCompleteWith activationState: WCSessionActivationState, error: Error?) {
        if let error {
            setLastError(error.localizedDescription)
        } else if activationState == .activated {
            setLastError(nil)
            _ = publishPendingContext()
        }
    }
    func sessionDidBecomeInactive(_ session: WCSession) {}
    func sessionDidDeactivate(_ session: WCSession) { session.activate() }
    func session(_ session: WCSession, didReceiveMessage message: [String: Any]) {
        if applyReceipt(from: message) { return }
        _ = enqueueAction(from: message)
    }
    func session(
        _ session: WCSession,
        didReceiveMessage message: [String: Any],
        replyHandler: @escaping ([String: Any]) -> Void
    ) {
        if applyReceipt(from: message) {
            replyHandler(["accepted": true])
            return
        }
        let accepted = enqueueAction(from: message)
        replyHandler(["accepted": accepted])
    }

    func session(_ session: WCSession, didReceiveUserInfo userInfo: [String: Any] = [:]) {
        if applyReceipt(from: userInfo) { return }
        // transferUserInfo is the durable counterpart to a reachable
        // sendMessage. It is what makes a pause made on the watch reach the
        // phone after a short disconnect or when the iPhone app is suspended.
        _ = enqueueAction(from: userInfo)
    }

    private func enqueueAction(from message: [String: Any]) -> Bool {
        if let data = try? JSONSerialization.data(withJSONObject: message),
           let runtime = try? JSONDecoder().decode(WatchRuntimeActionMessage.self, from: data), runtime.isValid {
            actionLock.lock(); defer { actionLock.unlock() }
            var seen = UserDefaults.standard.stringArray(forKey: seenActionIDsKey) ?? []
            guard !seen.contains(runtime.actionID) else { return true }
            seen.append(runtime.actionID)
            UserDefaults.standard.set(Array(seen.suffix(64)), forKey: seenActionIDsKey)
            guard let encoded = try? JSONEncoder().encode(runtime), let raw = String(data: encoded, encoding: .utf8) else { return false }
            pendingActions.append(raw)
            sendRuntimeReceipt(actionID: runtime.actionID, revision: runtime.baseRevision, result: "accepted")
            return true
        }

        // Older watch builds sent only an action string. Keep that wire shape
        // working while immediately normalising it into the versioned contract.
        guard let action = message["action"] as? String, supportedActions.contains(action) else { return false }
        let runtime = WatchRuntimeActionMessage(
            schemaVersion: 1, actionID: UUID().uuidString, action: action,
            baseRevision: UserDefaults.standard.object(forKey: "pausio.last_synced_revision") as? UInt64 ?? 0,
            occurredAt: ISO8601DateFormatter().string(from: Date())
        )
        guard let data = try? JSONEncoder().encode(runtime), let raw = String(data: data, encoding: .utf8) else { return false }
        actionLock.lock(); pendingActions.append(raw); actionLock.unlock()
        sendRuntimeReceipt(actionID: runtime.actionID, revision: runtime.baseRevision, result: "accepted")
        return true
    }

    private func applyReceipt(from message: [String: Any]) -> Bool {
        if message["kind"] as? String == "health" {
            UserDefaults.standard.set(message, forKey: healthKey)
            return true
        }
        guard message["kind"] as? String == "settings_receipt",
              let revision = revision(in: message["revision"]) else { return false }
        UserDefaults.standard.set(revision, forKey: "pausio.last_synced_revision")
        if UserDefaults.standard.object(forKey: queuedRevisionKey) as? UInt64 == revision {
            UserDefaults.standard.removeObject(forKey: pendingContextKey)
        }
        setLastError(nil)
        return true
    }

    private func sendRuntimeReceipt(actionID: String, revision: UInt64, result: String) {
        let receipt: [String: Any] = [
            "kind": "runtime_action", "action_id": actionID, "revision": revision,
            "result": result, "applied_at": ISO8601DateFormatter().string(from: Date()),
        ]
        let session = WCSession.default
        guard session.activationState == .activated else { return }
        _ = session.transferUserInfo(receipt)
        if session.isReachable { session.sendMessage(receipt, replyHandler: nil) { _ in } }
    }

    func storedHealth() -> [String: Any] { UserDefaults.standard.dictionary(forKey: healthKey) ?? [:] }

    private func revision(in value: Any?) -> UInt64? {
        if let value = value as? UInt64 { return value }
        if let value = value as? NSNumber { return value.uint64Value }
        return nil
    }

    private func setLastError(_ value: String?) {
        actionLock.lock(); defer { actionLock.unlock() }
        lastError = value
    }

    func storedLastError() -> String? {
        actionLock.lock(); defer { actionLock.unlock() }
        return lastError
    }
}

enum BridgeError: Error { case unavailable }

final class PausIOEyecarePlugin: Plugin {
    @objc func syncSettings(_ invoke: Invoke) throws {
        let envelope = try invoke.parseArgs(WatchEnvelope.self)
        let result = try PausIOWatchBridge.shared.sync(envelope)
        invoke.resolve(result)
    }

    @objc func sendTestNudge(_ invoke: Invoke) {
        // This confirms bridge dispatch only. It must never be presented as proof of a haptic.
        PausIOWatchBridge.shared.sendTestNudge { result in invoke.resolve(result) }
    }

    @objc func getStatus(_ invoke: Invoke) {
        PausIOWatchBridge.shared.activate()
        let session = WCSession.default
        let health = PausIOWatchBridge.shared.storedHealth()
        let state: String
        if !WCSession.isSupported() {
            state = "unavailable"
        } else if !session.isPaired {
            state = "unpaired"
        } else if !session.isWatchAppInstalled {
            state = "app_not_installed"
        } else if session.activationState != .activated {
            state = "activating"
        } else if !session.isReachable {
            state = "disconnected"
        } else {
            state = "connected"
        }
        let installed = session.isWatchAppInstalled
        invoke.resolve(BridgeStatus(
            platform: "ios",
            available: WCSession.isSupported(),
            paired: session.isPaired,
            appInstalled: session.isWatchAppInstalled,
            reachable: session.isReachable,
            lastSyncedRevision: UserDefaults.standard.object(forKey: "pausio.last_synced_revision") as? UInt64,
            lastError: PausIOWatchBridge.shared.storedLastError() ?? health["last_error"] as? String,
            lastQueuedRevision: UserDefaults.standard.object(forKey: "pausio.last_queued_revision") as? UInt64,
            connectionState: state,
            notificationPermission: health["notification_permission"] as? String,
            reminderPrecision: health["reminder_precision"] as? String,
            scheduleHorizonAt: health["schedule_horizon_at"] as? String,
            lastSuccessfulSyncAt: health["last_successful_sync_at"] as? String,
            appVersion: health["app_version"] as? String,
            capabilities: BridgeCapabilities(
                timerDisplay: installed,
                localReminders: installed,
                testHaptic: installed,
                remoteActions: installed,
                standalone: installed,
                complication: installed
            )
        ))
    }

    @objc func takePendingAction(_ invoke: Invoke) {
        invoke.resolve(PausIOWatchBridge.shared.takePendingAction() ?? "")
    }
}

@_cdecl("init_plugin_eyecare")
func initPluginEyecare() -> Plugin {
    PausIOWatchBridge.shared.activate()
    return PausIOEyecarePlugin()
}
