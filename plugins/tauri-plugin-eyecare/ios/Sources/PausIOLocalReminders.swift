import Foundation
import UserNotifications

/// One pre-computed reminder instant, as produced by `pausio-core`.
struct PausIOReminderSlot: Decodable {
    let at: Date
    let kind: String

    enum CodingKeys: String, CodingKey { case at, kind }
}

struct PausIOReminderPlanRequest: Decodable {
    let slots: [PausIOReminderSlot]
}

/// The phone's own break delivery.
///
/// iOS gives a suspended app no opportunity to run code at the moment a break
/// falls due, so the only thing that can reliably alert someone is a
/// notification registered ahead of time for an absolute instant. That is what
/// makes PausIO work on an iPhone with no Apple Watch, no pairing, and no
/// network — the reminders are already with the system before the app is
/// suspended.
///
/// The plan is recomputed and replaced wholesale on every state change rather
/// than patched, because a stale instant is worse than a missing one: it fires
/// a break the engine no longer believes in.
enum PausIOLocalReminders {
    private static let identifierPrefix = "pausio.phone."
    static let preBreakCategory = "pausio.phone.pre_break"
    static let breakDueCategory = "pausio.phone.break_due"
    static let startBreakAction = "pausio.phone.start_break"
    static let pauseAction = "pausio.phone.pause"

    /// iOS keeps at most 64 pending notifications per app and silently drops
    /// the rest, so the plan has to be budgeted rather than submitted blindly.
    private static let pendingLimit = 64

    static func registerCategories() {
        let start = UNNotificationAction(
            identifier: startBreakAction,
            title: NSLocalizedString("Start break", comment: ""),
            options: [.foreground]
        )
        let pause = UNNotificationAction(
            identifier: pauseAction,
            title: NSLocalizedString("Pause reminders", comment: "")
        )
        UNUserNotificationCenter.current().setNotificationCategories([
            UNNotificationCategory(
                identifier: preBreakCategory, actions: [pause], intentIdentifiers: []
            ),
            UNNotificationCategory(
                identifier: breakDueCategory, actions: [start, pause], intentIdentifiers: []
            ),
        ])
    }

    /// Replaces every pending PausIO reminder with `slots`.
    ///
    /// Only PausIO's own identifiers are removed, never another feature's.
    /// An empty `slots` therefore means "cancel everything", which is how a
    /// pause or a watch-only alert target clears the phone's plan.
    static func replace(
        with slots: [PausIOReminderSlot],
        completion: @escaping ([String: Any]) -> Void
    ) {
        let center = UNUserNotificationCenter.current()
        registerCategories()
        center.getPendingNotificationRequests { pending in
            let ours = pending.map(\.identifier).filter { $0.hasPrefix(identifierPrefix) }
            center.removePendingNotificationRequests(withIdentifiers: ours)

            guard !slots.isEmpty else {
                completion(report(scheduled: 0, horizon: nil, permission: nil, error: nil))
                return
            }

            center.getNotificationSettings { settings in
                switch settings.authorizationStatus {
                case _ where allowsDelivery(settings.authorizationStatus):
                    schedule(slots, center: center, completion: completion)
                case .notDetermined:
                    // Asking here rather than at launch means the prompt
                    // appears when reminders are actually being set up.
                    center.requestAuthorization(options: [.alert, .sound]) { granted, _ in
                        guard granted else {
                            completion(report(
                                scheduled: 0, horizon: nil, permission: "denied",
                                error: "Notifications are not permitted, so breaks cannot be announced"
                            ))
                            return
                        }
                        schedule(slots, center: center, completion: completion)
                    }
                default:
                    // A denial is a hard failure for a standalone phone: there
                    // is no second delivery path. Report it plainly.
                    completion(report(
                        scheduled: 0, horizon: nil, permission: "denied",
                        error: "Notifications are not permitted, so breaks cannot be announced"
                    ))
                }
            }
        }
    }

    private static func schedule(
        _ slots: [PausIOReminderSlot],
        center: UNUserNotificationCenter,
        completion: @escaping ([String: Any]) -> Void
    ) {
        let now = Date()
        let upcoming = slots.filter { $0.at > now }.sorted { $0.at < $1.at }
        // Break cues outrank their warnings: losing a pre-break warning is a
        // small regression, but losing the break itself defeats the product.
        // Reserve room for every break first, then spend what is left on
        // warnings — the same budgeting the watch companion uses.
        let breakSlots = upcoming.filter { $0.kind == "break_due" }
        let preBreakBudget = max(0, pendingLimit - breakSlots.count)
        var preBreaksUsed = 0

        var scheduled = 0
        var horizon: Date?
        var failure: String?
        let group = DispatchGroup()

        for (index, slot) in upcoming.enumerated() {
            if scheduled >= pendingLimit { break }
            let isPreBreak = slot.kind == "pre_break"
            if isPreBreak {
                if preBreaksUsed >= preBreakBudget { continue }
                preBreaksUsed += 1
            }

            let content = UNMutableNotificationContent()
            content.sound = .default
            if isPreBreak {
                content.title = NSLocalizedString("Break coming up", comment: "")
                content.body = NSLocalizedString(
                    "Find a good place to pause.", comment: ""
                )
                content.categoryIdentifier = preBreakCategory
            } else {
                content.title = NSLocalizedString("Time to rest your eyes", comment: "")
                content.body = NSLocalizedString(
                    "Look about 20 feet away for 20 seconds.", comment: ""
                )
                content.categoryIdentifier = breakDueCategory
            }

            // An absolute calendar trigger survives suspension and termination;
            // a time-interval trigger would be measured from registration.
            let components = Calendar.current.dateComponents(
                [.year, .month, .day, .hour, .minute, .second], from: slot.at
            )
            let request = UNNotificationRequest(
                identifier: "\(identifierPrefix)\(index).\(slot.kind)",
                content: content,
                trigger: UNCalendarNotificationTrigger(dateMatching: components, repeats: false)
            )
            group.enter()
            center.add(request) { error in
                if let error {
                    failure = error.localizedDescription
                } else {
                    scheduled += 1
                    if !isPreBreak, horizon.map({ slot.at > $0 }) ?? true { horizon = slot.at }
                }
                group.leave()
            }
        }

        group.notify(queue: .main) {
            if scheduled == 0, failure == nil {
                failure = "No reminders could be scheduled"
            }
            completion(report(
                scheduled: scheduled, horizon: horizon, permission: "granted", error: failure
            ))
        }
    }

    static func permissionState(_ completion: @escaping (String) -> Void) {
        UNUserNotificationCenter.current().getNotificationSettings { settings in
            completion(describe(settings.authorizationStatus))
        }
    }

    static func requestPermission(_ completion: @escaping (String) -> Void) {
        let center = UNUserNotificationCenter.current()
        center.getNotificationSettings { settings in
            guard settings.authorizationStatus == .notDetermined else {
                completion(describe(settings.authorizationStatus))
                return
            }
            center.requestAuthorization(options: [.alert, .sound]) { _, _ in
                center.getNotificationSettings { updated in
                    completion(describe(updated.authorizationStatus))
                }
            }
        }
    }

    /// Posts a reminder straight away so a person can confirm that standalone
    /// delivery works on their device without waiting for a real break.
    static func postTest(_ completion: @escaping (String) -> Void) {
        let center = UNUserNotificationCenter.current()
        registerCategories()
        center.getNotificationSettings { settings in
            guard allowsDelivery(settings.authorizationStatus) else {
                completion("unavailable")
                return
            }
            let content = UNMutableNotificationContent()
            content.title = NSLocalizedString("Time to rest your eyes", comment: "")
            content.body = NSLocalizedString(
                "Look about 20 feet away for 20 seconds.", comment: ""
            )
            content.sound = .default
            content.categoryIdentifier = breakDueCategory
            center.add(UNNotificationRequest(
                identifier: "\(identifierPrefix)test",
                content: content,
                trigger: UNTimeIntervalNotificationTrigger(timeInterval: 1, repeats: false)
            )) { error in
                completion(error == nil ? "delivered" : "unavailable")
            }
        }
    }

    /// Whether this status will actually put a reminder in front of someone.
    ///
    /// `.ephemeral` only exists from iOS 14, so it is matched by raw value to
    /// keep one predicate rather than scattering availability checks through
    /// every caller.
    private static func allowsDelivery(_ status: UNAuthorizationStatus) -> Bool {
        if status == .authorized || status == .provisional { return true }
        if #available(iOS 14.0, *) { return status == .ephemeral }
        return false
    }

    private static func describe(_ status: UNAuthorizationStatus) -> String {
        if allowsDelivery(status) { return "granted" }
        switch status {
        case .denied: return "denied"
        case .notDetermined: return "not_determined"
        default: return "unknown"
        }
    }

    private static func report(
        scheduled: Int, horizon: Date?, permission: String?, error: String?
    ) -> [String: Any] {
        var payload: [String: Any] = [
            "scheduled": scheduled,
            // iOS calendar triggers are exact; there is no inexact mode to
            // downgrade to, unlike Android's alarm scheduling.
            "precision": "exact",
        ]
        if let horizon { payload["horizon_at"] = ISO8601DateFormatter().string(from: horizon) }
        if let permission { payload["permission"] = permission }
        if let error { payload["last_error"] = error }
        return payload
    }
}
