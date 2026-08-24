#if os(watchOS)
import SwiftUI
import UserNotifications
import WatchConnectivity
import WatchKit
import WidgetKit

private func L(_ key: String) -> String { NSLocalizedString(key, comment: "") }

@main struct PausIOWatchApp: App {
    @StateObject private var bridge = WatchBridge()

    var body: some Scene {
        WindowGroup {
            TimelineView(.periodic(from: Date(), by: 1)) { timeline in
                WatchDashboard(
                    bridge: bridge,
                    countdown: bridge.countdown(at: timeline.date),
                    isBreak: bridge.isBreak(at: timeline.date)
                )
                .onAppear { bridge.activate() }
            }
        }
    }
}

private struct WatchDashboard: View {
    @ObservedObject var bridge: WatchBridge
    let countdown: WatchCountdown?
    let isBreak: Bool

    private var accent: Color { isBreak ? .green : .blue }

    var body: some View {
        GeometryReader { proxy in
            ScrollView(.vertical, showsIndicators: false) {
                VStack(spacing: 6) {
                    watchHeader
                    watchFace(diameter: watchFaceDiameter(in: proxy.size))
                    status
                    controls
                }
                .padding(.horizontal, 10)
                .padding(.vertical, 4)
                // The content is exactly one viewport tall. This preserves the
                // native crown bounce while ensuring it always settles back at
                // the centered position instead of a cropped scroll offset.
                .frame(maxWidth: .infinity, minHeight: proxy.size.height, alignment: .center)
            }
            .pausioWatchBounce()
            .background(
                RadialGradient(
                    colors: [accent.opacity(0.14), .clear],
                    center: .center,
                    startRadius: 8,
                    endRadius: proxy.size.width * 0.7
                )
            )
        }
        .accessibilityElement(children: .contain)
    }

    private func watchFaceDiameter(in size: CGSize) -> CGFloat {
        // A 112-point dial looks great in isolation but overflows compact
        // watches once the status and action controls are included.
        min(96, max(76, min(size.width * 0.48, size.height * 0.42)))
    }

    private var watchHeader: some View {
        HStack(spacing: 6) {
            Circle()
                .fill(accent)
                .frame(width: 6, height: 6)
                .shadow(color: accent.opacity(0.7), radius: 3)
            Text("PAUSIO")
                .font(.system(size: 10, weight: .bold, design: .rounded))
                .tracking(1.25)
                .foregroundStyle(.secondary)
            Spacer(minLength: 4)
            Image(systemName: bridge.isReachable ? "iphone" : "iphone.slash")
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(bridge.isReachable ? accent : Color.secondary)
                .accessibilityLabel(bridge.isReachable ? "iPhone connected" : "iPhone is not reachable")
        }
        .padding(.horizontal, 4)
    }

    @ViewBuilder private func watchFace(diameter: CGFloat) -> some View {
        if bridge.isDormant {
            StateRing(
                symbol: "moon.zzz.fill",
                tint: .indigo,
                diameter: diameter,
                accessibilityLabel: L("outside_hours")
            )
        } else if bridge.isPaused {
            StateRing(
                symbol: "pause.fill",
                tint: .gray,
                diameter: diameter,
                accessibilityLabel: L("paused")
            )
        } else if bridge.isBreakDue {
            StateRing(
                symbol: "eye.fill",
                tint: .blue,
                diameter: diameter,
                accessibilityLabel: L("time_for_break")
            )
        } else if let countdown {
            CountdownRing(countdown: countdown, tint: accent, diameter: diameter)
        } else {
            ConnectionRing(hasSettings: bridge.revision > 0, diameter: diameter)
        }
    }

    private var status: some View {
        let fallback: String
        if bridge.isDormant {
            fallback = L("outside_hours")
        } else if bridge.isPaused {
            fallback = L("paused")
        } else if bridge.isBreakDue {
            fallback = L("time_for_break")
        } else if let countdown {
            fallback = countdown.label
        } else {
            fallback = bridge.revision > 0 ? "Syncing with iPhone" : "Open PausIO on iPhone"
        }
        return StatusText(text: bridge.statusText(default: fallback))
    }

    @ViewBuilder private var controls: some View {
        if bridge.isDormant {
            EmptyView()
        } else if bridge.isPaused {
            WatchControlButton(
                symbol: "play.fill",
                label: L("resume"),
                hint: "Resumes the eye-rest timer",
                tint: .green,
                isPending: bridge.isActionPending
            ) { bridge.sendAction("resume") }
        } else if bridge.isBreakDue {
            WatchControlButton(
                symbol: "eye.fill",
                label: L("start_break"),
                hint: "Starts the eye break on your iPhone",
                tint: .blue,
                isPending: bridge.isActionPending
            ) { bridge.sendAction("take_break_now") }
        } else if countdown != nil, isBreak {
            WatchControlButton(
                symbol: "checkmark",
                label: L("finish_break"),
                hint: "Ends this eye break and starts the next work interval",
                tint: .green,
                isPending: bridge.isActionPending
            ) { bridge.sendAction("skip_break") }
        } else if countdown != nil {
            HStack(spacing: 12) {
                WatchControlButton(
                    symbol: "pause.fill",
                    label: L("pause"),
                    hint: "Pauses the eye-rest timer",
                    tint: .gray,
                    isPending: bridge.isActionPending
                ) { bridge.sendAction("pause") }
                WatchControlButton(
                    symbol: "eye.fill",
                    label: L("take_break_now"),
                    hint: "Starts an eye break immediately",
                    tint: .blue,
                    isPending: bridge.isActionPending
                ) { bridge.sendAction("take_break_now") }
            }
        }
    }
}

private extension View {
    @ViewBuilder func pausioWatchBounce() -> some View {
        if #available(watchOS 9.4, *) {
            self.scrollBounceBehavior(.always, axes: .vertical)
        } else {
            self
        }
    }
}

private struct StatusText: View {
    let text: String

    var body: some View {
        Text(text)
            .font(.system(size: 11, weight: .medium, design: .rounded))
            .foregroundStyle(.secondary)
            .multilineTextAlignment(.center)
            .lineLimit(2)
            .minimumScaleFactor(0.8)
            .accessibilityLabel(text)
    }
}

private struct StateRing: View {
    let symbol: String
    let tint: Color
    let diameter: CGFloat
    let accessibilityLabel: String

    var body: some View {
        ZStack {
            Circle()
                .fill(tint.opacity(0.08))
            Circle()
                .stroke(tint.opacity(0.25), lineWidth: 7)
            Image(systemName: symbol)
                .font(.system(size: diameter * 0.24, weight: .semibold))
                .foregroundStyle(tint)
        }
        .frame(width: diameter, height: diameter)
        .accessibilityLabel(accessibilityLabel)
    }
}

private struct ConnectionRing: View {
    let hasSettings: Bool
    let diameter: CGFloat

    var body: some View {
        ZStack {
            Circle().fill(.gray.opacity(0.08))
            Circle().stroke(.gray.opacity(0.22), lineWidth: 7)
            if hasSettings {
                ProgressView().controlSize(.large)
            } else {
                Image(systemName: "iphone.and.arrow.forward")
                    .font(.system(size: diameter * 0.24, weight: .semibold))
                    .foregroundStyle(.secondary)
            }
        }
        .frame(width: diameter, height: diameter)
        .accessibilityLabel(hasSettings ? "Synchronizing with iPhone" : "Waiting for iPhone settings")
    }
}

private struct CountdownRing: View {
    let countdown: WatchCountdown
    let tint: Color
    let diameter: CGFloat

    var body: some View {
        ZStack {
            Circle().fill(tint.opacity(0.07))
            Circle().stroke(tint.opacity(0.18), lineWidth: 7)
            Circle()
                .trim(from: 0, to: countdown.remainingFraction)
                .stroke(
                    AngularGradient(colors: [tint.opacity(0.68), tint], center: .center),
                    style: StrokeStyle(lineWidth: 7, lineCap: .round)
                )
                .rotationEffect(.degrees(-90))
                .shadow(color: tint.opacity(0.35), radius: 3)
            Text(countdown.remainingText)
                .font(.system(size: diameter * 0.25, weight: .bold, design: .rounded))
                .monospacedDigit()
        }
        .frame(width: diameter, height: diameter)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(countdown.label)
        .accessibilityValue(countdown.accessibilityRemaining)
    }
}

private struct WatchControlButton: View {
    let symbol: String
    let label: String
    let hint: String
    let tint: Color
    let isPending: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Group {
                if isPending {
                    ProgressView().controlSize(.small)
                } else {
                    Image(systemName: symbol)
                }
            }
            .font(.system(size: 16, weight: .semibold))
            .frame(width: 44, height: 44)
            .foregroundStyle(tint == .gray ? Color.primary : Color.black)
            .background(
                Circle()
                    .fill(tint == .gray ? Color.gray.opacity(0.24) : tint)
                    .shadow(color: tint == .gray ? .clear : tint.opacity(0.32), radius: 5, y: 2)
            )
        }
        .buttonStyle(.plain)
        .disabled(isPending)
        .accessibilityLabel(label)
        .accessibilityHint(hint)
    }
}

private struct WatchCountdown {
    let remainingSeconds: Int
    let remainingFraction: Double
    let label: String

    var remainingText: String {
        String(format: "%02d:%02d", remainingSeconds / 60, remainingSeconds % 60)
    }

    var accessibilityRemaining: String {
        let minutes = remainingSeconds / 60
        let seconds = remainingSeconds % 60
        return "\(minutes) minutes, \(seconds) seconds remaining"
    }
}

@MainActor final class WatchBridge: NSObject, ObservableObject, @preconcurrency WCSessionDelegate, UNUserNotificationCenterDelegate, @preconcurrency WKExtendedRuntimeSessionDelegate {
    @Published private(set) var revision: UInt64 = 0
    @Published private(set) var isPaused = false
    @Published private(set) var isReachable = false
    @Published private(set) var isActionPending = false
    @Published private(set) var commandStatus: String?
    @Published private(set) var connectionStatus: String?

    private let storageKey = "pausio.watch.settings.v1"
    private let scheduleDeadlineKey = "pausio.watch.schedule-deadline.v1"
    private var scheduleDeadline: Date?
    private var workIntervalSeconds = 0
    private var standardWorkIntervalSeconds = 0
    private var isBreakActive = false
    private var currentPhase: WatchTimerPhase?
    private var settings: WatchSettingsEnvelope?
    private var hasSettings = false
    private var extendedRuntimeSession: WKExtendedRuntimeSession?

    func activate() {
        restore()
        UNUserNotificationCenter.current().delegate = self
        Task { await WatchReminderScheduler.registerCategories() }
        guard WCSession.isSupported() else {
            isReachable = false
            connectionStatus = "iPhone connection unavailable"
            return
        }
        WCSession.default.delegate = self
        WCSession.default.activate()
        isReachable = WCSession.default.isReachable
        if WCSession.default.activationState != .activated {
            connectionStatus = "Connecting to iPhone"
        }
    }

    func session(_ session: WCSession, activationDidCompleteWith activationState: WCSessionActivationState, error: Error?) {
        Task { @MainActor in
            self.isReachable = activationState == .activated && session.isReachable
            if error != nil {
                self.connectionStatus = "iPhone connection unavailable"
                self.commandStatus = nil
                return
            }
            guard activationState == .activated else {
                self.connectionStatus = "Connecting to iPhone"
                return
            }
            self.connectionStatus = nil
            self.apply(session.receivedApplicationContext)
        }
    }

    func sessionReachabilityDidChange(_ session: WCSession) {
        Task { @MainActor in
            self.isReachable = session.isReachable
            if session.isReachable, self.commandStatus == "Open PausIO on iPhone" {
                self.commandStatus = nil
            }
        }
    }

    func session(_ session: WCSession, didReceiveApplicationContext applicationContext: [String: Any]) {
        Task { @MainActor in self.apply(applicationContext) }
    }

    func session(_ session: WCSession, didReceiveMessage message: [String: Any]) {
        switch message["kind"] as? String {
        case "settings":
            if let envelope = message["envelope"] as? [String: Any] {
                apply(envelope)
            }
        case "test":
            Task { @MainActor in _ = await self.performTestNudge() }
        case "runtime_action":
            Task { @MainActor in
                self.isActionPending = false
                self.commandStatus = L("updated_on_phone")
            }
        default: break
        }
    }

    func session(
        _ session: WCSession,
        didReceiveMessage message: [String: Any],
        replyHandler: @escaping ([String: Any]) -> Void
    ) {
        switch message["kind"] as? String {
        case "settings":
            guard let envelope = message["envelope"] as? [String: Any] else {
                replyHandler(["accepted": false])
                return
            }
            Task { @MainActor in
                self.apply(envelope)
                replyHandler(["accepted": true, "revision": self.revision])
            }
        case "test":
            guard let eventID = message["event_id"] as? String, !eventID.isEmpty else {
                replyHandler(["accepted": false])
                return
            }
            Task { @MainActor in
                let notificationScheduled = await self.performTestNudge()
                replyHandler([
                    "accepted": true,
                    "event_id": eventID,
                    "notification_scheduled": notificationScheduled,
                ])
            }
        default:
            replyHandler(["accepted": false])
        }
    }

    func session(_ session: WCSession, didReceiveUserInfo userInfo: [String: Any] = [:]) {
        guard userInfo["kind"] as? String == "runtime_action" else { return }
        Task { @MainActor in
            self.isActionPending = false
            self.commandStatus = L("updated_on_phone")
        }
    }

    func sendAction(_ action: String) {
        let session = WCSession.default
        guard !isActionPending else { return }
        applyLocalAction(action)
        guard session.activationState == .activated else {
            commandStatus = "Will update on iPhone when connected"
            playHaptic(.click)
            return
        }
        isActionPending = true
        commandStatus = "Updating…"
        let command = WatchRuntimeActionV1(
            schemaVersion: 1,
            actionID: UUID(),
            action: WatchRuntimeAction(rawValue: action) ?? .pause,
            baseRevision: revision,
            occurredAt: Date()
        )
        guard let encoded = try? JSONEncoder.pausio.encode(command),
              let message = try? JSONSerialization.jsonObject(with: encoded) as? [String: Any] else {
            failPendingAction("Action unavailable")
            return
        }
        // Queue the command as well as sending it interactively. The phone
        // de-duplicates action IDs, so this gives immediate feedback while
        // still reconciling a pause if either app goes into the background.
        _ = session.transferUserInfo(message)
        guard session.isReachable else {
            isActionPending = false
            commandStatus = "Will update on iPhone when connected"
            playHaptic(.click)
            return
        }
        session.sendMessage(message) { response in
            Task { @MainActor in
                guard response["accepted"] as? Bool == true else {
                    self.failPendingAction("Action unavailable")
                    return
                }
                self.playHaptic(.click)
                self.startActionTimeout()
            }
        } errorHandler: { _ in
            Task { @MainActor in self.failPendingAction("Open PausIO on iPhone") }
        }
    }

    private func restore() {
        if let data = UserDefaults.standard.data(forKey: storageKey) {
            if apply(data, scheduleDeadline: UserDefaults.standard.object(forKey: scheduleDeadlineKey) as? Date) {
                return
            }
        }
        let defaults = WatchSettingsEnvelope.localDefaults()
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        guard let data = try? encoder.encode(defaults) else { return }
        _ = apply(data)
    }

    fileprivate func countdown(at now: Date) -> WatchCountdown? {
        advanceLocalPhaseIfNeeded(at: now)
        guard let scheduleDeadline, workIntervalSeconds > 0 else { return nil }
        let deadline: Date
        let duration: Int
        let breakDisplay: Bool
        if isBreakActive, now >= scheduleDeadline, standardWorkIntervalSeconds > 0 {
            // Break completion is deterministic. Project the next work interval locally
            // while waiting for the phone's authoritative state revision.
            deadline = scheduleDeadline.addingTimeInterval(TimeInterval(standardWorkIntervalSeconds))
            duration = standardWorkIntervalSeconds
            breakDisplay = false
        } else {
            deadline = scheduleDeadline
            duration = workIntervalSeconds
            breakDisplay = isBreakActive
        }
        guard now < deadline else { return nil }
        let scheduleStart = deadline.addingTimeInterval(-TimeInterval(duration))
        let remaining = WatchScheduleStore.remainingSeconds(
            startedAt: scheduleStart,
            durationSeconds: duration,
            now: now
        )
        return WatchCountdown(
            remainingSeconds: remaining,
            remainingFraction: WatchScheduleStore.remainingFraction(
                startedAt: scheduleStart,
                durationSeconds: duration,
                now: now
            ),
            label: breakDisplay ? L("look_into_distance") : L("until_next_pause")
        )
    }

    fileprivate func isBreak(at now: Date) -> Bool {
        isBreakActive && (scheduleDeadline.map { now < $0 } ?? false)
    }

    fileprivate var isBreakDue: Bool {
        if case .some(.breakDue) = currentPhase { return true }
        return false
    }

    fileprivate var isDormant: Bool {
        if case .some(.dormant) = currentPhase { return true }
        return false
    }

    fileprivate func statusText(default defaultText: String) -> String {
        commandStatus ?? connectionStatus ?? defaultText
    }

    private func apply(_ context: [String: Any]) {
        guard JSONSerialization.isValidJSONObject(context),
              let data = try? JSONSerialization.data(withJSONObject: context) else { return }
        if apply(data, scheduleDeadline: nil) {
            sendReceipt(for: revision)
        }
    }

    @discardableResult private func apply(_ data: Data, scheduleDeadline: Date? = nil) -> Bool {
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        guard let value = try? decoder.decode(WatchSettingsEnvelope.self, from: data),
              WatchScheduleStore.shouldApply(value, over: settings) else { return false }
        let hadState = hasSettings
        let wasPaused = isPaused
        let wasBreakActive = isBreakActive
        revision = value.revision
        hasSettings = true
        isActionPending = false
        commandStatus = nil
        UserDefaults.standard.set(data, forKey: storageKey)
        settings = value
        currentPhase = value.phase
        if value.paused || value.phase?.suspendsSchedule == true {
            self.scheduleDeadline = nil
            workIntervalSeconds = 0
            UserDefaults.standard.removeObject(forKey: scheduleDeadlineKey)
            isPaused = true
            Task { await WatchReminderScheduler.replace(with: value, firstBreakAt: nil, schedulingEnabled: false) }
            if hadState && !wasPaused { playHaptic(.stop) }
            return true
        }
        isPaused = false
        isBreakActive = value.phase?.isBreakInProgress ?? value.breakActive ?? false
        standardWorkIntervalSeconds = value.workIntervalSeconds
        if hadState {
            if !wasBreakActive && isBreakActive {
                playHaptic(.start)
            } else if wasBreakActive && !isBreakActive {
                playHaptic(.success)
            } else if wasPaused {
                playHaptic(.start)
            }
        }
        let duration = value.phase?.duration(in: value)
            ?? (isBreakActive ? ((value.breakKind ?? value.phase?.breakKind) == "long" ? value.longBreakSeconds : value.shortBreakSeconds) : value.workIntervalSeconds)
        guard duration > 0 else {
            self.scheduleDeadline = nil
            workIntervalSeconds = 0
            UserDefaults.standard.removeObject(forKey: scheduleDeadlineKey)
            Task { await WatchReminderScheduler.replace(with: value, firstBreakAt: nil, schedulingEnabled: false) }
            return true
        }
        let deadline = value.phaseDeadlineAt ?? scheduleDeadline ?? value.nextBreakAt ?? Date().addingTimeInterval(TimeInterval(duration))
        self.scheduleDeadline = deadline
        workIntervalSeconds = duration
        UserDefaults.standard.set(deadline, forKey: scheduleDeadlineKey)
        let firstOfflineBreak = isBreakActive
            ? deadline.addingTimeInterval(TimeInterval(standardWorkIntervalSeconds))
            : deadline
        Task { await WatchReminderScheduler.replace(with: value, firstBreakAt: firstOfflineBreak) }
        publishComplicationState()
        reinforceCurrentInterval()
        return true
    }

    private func startActionTimeout() {
        Task {
            try? await Task.sleep(for: .seconds(4))
            guard self.isActionPending else { return }
            self.isActionPending = false
            self.commandStatus = "No update from iPhone"
            self.playHaptic(.failure)
        }
    }

    private func failPendingAction(_ message: String) {
        isActionPending = false
        commandStatus = message
        playHaptic(.failure)
    }

    /// Applies every control locally first. When the iPhone is unavailable the
    /// action intentionally stays local; the next settings revision reconciles it.
    private func applyLocalAction(_ rawAction: String) {
        let now = Date()
        guard let action = WatchRuntimeAction(rawValue: rawAction) else { return }
        switch action {
        case .pause:
            isPaused = true
            currentPhase = .paused
            scheduleDeadline = nil
            workIntervalSeconds = 0
        case .resume:
            isPaused = false
            currentPhase = .working
            isBreakActive = false
            workIntervalSeconds = standardWorkIntervalSeconds
            scheduleDeadline = now.addingTimeInterval(TimeInterval(standardWorkIntervalSeconds))
        case .takeBreakNow:
            isPaused = false
            currentPhase = .breaking(kind: nil)
            isBreakActive = true
            workIntervalSeconds = 20
            scheduleDeadline = now.addingTimeInterval(20)
        case .skipBreak:
            isPaused = false
            currentPhase = .working
            isBreakActive = false
            workIntervalSeconds = standardWorkIntervalSeconds
            scheduleDeadline = now.addingTimeInterval(TimeInterval(standardWorkIntervalSeconds))
        }
        if let scheduleDeadline { UserDefaults.standard.set(scheduleDeadline, forKey: scheduleDeadlineKey) }
        else { UserDefaults.standard.removeObject(forKey: scheduleDeadlineKey) }
        if let settings {
            let firstBreak = isBreakActive
                ? scheduleDeadline.map { $0.addingTimeInterval(TimeInterval(standardWorkIntervalSeconds)) }
                : scheduleDeadline
            Task {
                await WatchReminderScheduler.replace(
                    with: settings,
                    firstBreakAt: firstBreak,
                    schedulingEnabled: !isPaused
                )
            }
        }
        publishComplicationState()
    }

    /// Notification delivery must be able to advance the timer without an
    /// iPhone process. This persisted state machine is the local authority
    /// until a newer phone-owned revision arrives.
    private func advanceLocalPhaseIfNeeded(at now: Date) {
        guard !isPaused, let deadline = scheduleDeadline, now >= deadline else { return }
        switch currentPhase {
        case .some(.breaking):
            currentPhase = .working
            isBreakActive = false
            workIntervalSeconds = standardWorkIntervalSeconds
            scheduleDeadline = now.addingTimeInterval(TimeInterval(standardWorkIntervalSeconds))
        case .some(.working), .some(.preBreak), nil:
            currentPhase = .breakDue(kind: nil)
            isBreakActive = false
            workIntervalSeconds = 0
            scheduleDeadline = nil
            playHaptic(.notification)
        default:
            return
        }
        if let scheduleDeadline { UserDefaults.standard.set(scheduleDeadline, forKey: scheduleDeadlineKey) }
        else { UserDefaults.standard.removeObject(forKey: scheduleDeadlineKey) }
        publishComplicationState()
    }

    private func playHaptic(_ type: WKHapticType) {
        WKInterfaceDevice.current().play(type)
    }

    private func publishComplicationState() {
        let store = UserDefaults(suiteName: "group.com.pausio.app")
        let title = isDormant
            ? L("outside_hours")
            : (isPaused ? L("paused") : (isBreakActive ? L("look_into_distance") : L("until_next_pause")))
        store?.set(title, forKey: "pausio.complication.title")
        store?.set(scheduleDeadline, forKey: "pausio.complication.deadline")
        store?.set(workIntervalSeconds, forKey: "pausio.complication.duration")
        WidgetCenter.shared.reloadAllTimelines()
    }

    /// A self-care runtime session reinforces the currently visible interval
    /// only. Local notifications remain the authoritative all-day mechanism.
    private func reinforceCurrentInterval() {
        guard extendedRuntimeSession == nil else { return }
        let session = WKExtendedRuntimeSession()
        session.delegate = self
        extendedRuntimeSession = session
        session.start()
    }

    func extendedRuntimeSessionDidStart(_ extendedRuntimeSession: WKExtendedRuntimeSession) {}

    func extendedRuntimeSessionWillExpire(_ extendedRuntimeSession: WKExtendedRuntimeSession) {
        self.extendedRuntimeSession = nil
    }

    func extendedRuntimeSession(
        _ extendedRuntimeSession: WKExtendedRuntimeSession,
        didInvalidateWith reason: WKExtendedRuntimeSessionInvalidationReason,
        error: Error?
    ) {
        self.extendedRuntimeSession = nil
    }

    private func performTestNudge() async -> Bool {
        playHaptic(.notification)
        return await schedulePreviewNotification(after: 1)
    }

    private func sendReceipt(for revision: UInt64) {
        guard revision > 0 else { return }
        let receipt: [String: Any] = ["kind": "settings_receipt", "revision": revision]
        let session = WCSession.default
        guard session.activationState == .activated else {
            _ = session.transferUserInfo(receipt)
            return
        }
        if session.isReachable {
            session.sendMessage(receipt, replyHandler: nil) { _ in
                _ = session.transferUserInfo(receipt)
            }
        } else {
            _ = session.transferUserInfo(receipt)
        }
    }

    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification,
        withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void
    ) {
        completionHandler([.banner, .sound])
    }

    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse,
        withCompletionHandler completionHandler: @escaping () -> Void
    ) {
        let action: String? = switch response.actionIdentifier {
        case WatchReminderScheduler.startBreakAction: "take_break_now"
        case WatchReminderScheduler.pauseAction: "pause"
        case WatchReminderScheduler.endBreakAction: "skip_break"
        default: nil
        }
        if let action {
            Task { @MainActor in self.sendAction(action) }
        }
        completionHandler()
    }
}

private extension JSONEncoder {
    static var pausio: JSONEncoder {
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        return encoder
    }
}

private enum WatchHealthPublisher {
    static func publish(horizon: Date?, lastError: String?) async {
        let notificationSettings = await UNUserNotificationCenter.current().notificationSettings()
        let permission: String = switch notificationSettings.authorizationStatus {
        case .authorized, .provisional, .ephemeral: "granted"
        case .denied: "denied"
        default: "not_determined"
        }
        var payload: [String: Any] = [
            "kind": "health",
            "schema_version": 1,
            "app_version": Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "unknown",
            "notification_permission": permission,
            "reminder_precision": "exact",
            "last_successful_sync_at": ISO8601DateFormatter().string(from: Date()),
        ]
        if let horizon { payload["schedule_horizon_at"] = horizon.ISO8601Format() }
        if let lastError { payload["last_error"] = lastError }
        let session = WCSession.default
        guard WCSession.isSupported(), session.activationState == .activated else { return }
        _ = session.transferUserInfo(payload)
        if session.isReachable { session.sendMessage(payload, replyHandler: nil) { _ in } }
    }
}

/// Offline-first watch reminders. These are scheduled on the watch itself, so
/// they remain useful if the iPhone is unreachable. The settings envelope and
/// generated notifications contain no desktop activity, account, or content.
private enum WatchReminderScheduler {
    private static let prefix = "pausio.reminder."
    static let startBreakAction = "pausio.start_break"
    static let pauseAction = "pausio.pause"
    static let endBreakAction = "pausio.end_break"
    private static let preBreakCategory = "pausio.pre_break"
    private static let breakDueCategory = "pausio.break_due"

    static func registerCategories() async {
        let pause = UNNotificationAction(identifier: pauseAction, title: L("pause_reminders"))
        let start = UNNotificationAction(identifier: startBreakAction, title: L("start_break"), options: [.foreground])
        let end = UNNotificationAction(identifier: endBreakAction, title: L("end_break"), options: [.foreground])
        let preBreak = UNNotificationCategory(identifier: preBreakCategory, actions: [pause], intentIdentifiers: [])
        let breakDue = UNNotificationCategory(identifier: breakDueCategory, actions: [start, end], intentIdentifiers: [])
        UNUserNotificationCenter.current().setNotificationCategories([preBreak, breakDue])
    }

    static func replace(
        with settings: WatchSettingsEnvelope,
        firstBreakAt: Date?,
        schedulingEnabled: Bool = true
    ) async {
        let center = UNUserNotificationCenter.current()
        await registerCategories()
        let existing = await center.pendingNotificationRequests()
        center.removePendingNotificationRequests(withIdentifiers: existing
            .map(\.identifier)
            .filter { $0.hasPrefix(prefix) })
        guard !settings.paused, schedulingEnabled else {
            await WatchHealthPublisher.publish(horizon: nil, lastError: nil)
            return
        }

        let notificationSettings = await center.notificationSettings()
        if notificationSettings.authorizationStatus == .notDetermined {
            _ = try? await center.requestAuthorization(options: [.alert, .sound])
        }
        guard (await center.notificationSettings()).authorizationStatus == .authorized else {
            await WatchHealthPublisher.publish(horizon: nil, lastError: "Notifications are not permitted")
            return
        }

        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: settings.timezone) ?? .current
        // Keep the whole plan within watchOS's 64 pending-notification limit.
        // Reserve pre-break cues only when the remaining breaks still cover at
        // least eight hours; otherwise the break-due notification has priority.
        let requiredEightHourBreaks = Int(ceil((8 * 60 * 60) / Double(settings.workIntervalSeconds)))
        let preBreakBudget = settings.preBreakSeconds > 0 ? min(32, max(0, 64 - requiredEightHourBreaks)) : 0
        let breakBudget = 64 - preBreakBudget
        var horizon: Date?
        var scheduledRequests = 0
        var failure: String?
        for (index, date) in WatchScheduleStore.reminderDates(
            for: settings,
            firstBreakAt: firstBreakAt,
            now: Date(),
            limit: breakBudget
        ).enumerated() {
            if index < preBreakBudget {
                let preDate = date.addingTimeInterval(-TimeInterval(settings.preBreakSeconds))
                if preDate > Date() {
                    let preContent = UNMutableNotificationContent()
                    preContent.title = L("upcoming_break_title")
                    preContent.body = L("upcoming_break_body")
                    preContent.sound = .default
                    preContent.categoryIdentifier = preBreakCategory
                    let preComponents = calendar.dateComponents([.calendar, .timeZone, .year, .month, .day, .hour, .minute, .second], from: preDate)
                    do {
                        try await center.add(UNNotificationRequest(
                            identifier: "\(prefix)\(settings.revision).pre.\(index)",
                            content: preContent,
                            trigger: UNCalendarNotificationTrigger(dateMatching: preComponents, repeats: false)
                        ))
                        scheduledRequests += 1
                    } catch { failure = error.localizedDescription }
                }
            }
            let content = UNMutableNotificationContent()
            content.title = L("break_due_title")
            content.body = L("break_due_body")
            content.sound = .default
            content.categoryIdentifier = breakDueCategory
            let components = calendar.dateComponents([.calendar, .timeZone, .year, .month, .day, .hour, .minute, .second], from: date)
            let request = UNNotificationRequest(
                identifier: "\(prefix)\(settings.revision).\(index)",
                content: content,
                trigger: UNCalendarNotificationTrigger(dateMatching: components, repeats: false)
            )
            do {
                try await center.add(request)
                scheduledRequests += 1
                horizon = date
            } catch { failure = error.localizedDescription }
        }
        if horizon.map({ $0 < Date().addingTimeInterval(8 * 60 * 60) }) == true, failure == nil {
            failure = "The current interval cannot fit eight hours within watchOS's notification limit"
        }
        if scheduledRequests == 0, failure == nil { failure = "No reminders were scheduled" }
        await WatchHealthPublisher.publish(horizon: horizon, lastError: failure)
    }
}

func schedulePreviewNotification(after seconds: TimeInterval) async -> Bool {
    let center = UNUserNotificationCenter.current()
    var settings = await center.notificationSettings()
    if settings.authorizationStatus == .notDetermined {
        do {
            _ = try await center.requestAuthorization(options: [.alert, .sound])
        } catch {
            return false
        }
        settings = await center.notificationSettings()
    }
    guard settings.authorizationStatus == .authorized else { return false }
    let content = UNMutableNotificationContent()
    content.title = "Time to rest your eyes"
    content.body = "Look about 20 feet away for 20 seconds."
    content.sound = .default
    do {
        try await center.add(UNNotificationRequest(identifier: "pausio.preview", content: content, trigger: UNTimeIntervalNotificationTrigger(timeInterval: max(1, seconds), repeats: false)))
        return true
    } catch {
        return false
    }
}
#endif
