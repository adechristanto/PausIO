import XCTest
@testable import PausIOWatch

final class WatchScheduleStoreTests: XCTestCase {
    func testNewestRevisionWins() async throws {
        let store = WatchScheduleStore()
        let base = settings(revision: 2)
        let firstApply = await store.apply(base)
        let secondApply = await store.apply(base)
        let next = await store.nextBreak(after: Date(timeIntervalSince1970: 0))
        XCTAssertTrue(firstApply)
        XCTAssertFalse(secondApply)
        XCTAssertEqual(next, Date(timeIntervalSince1970: 1200))
    }

    func testNewerPhoneTimestampRecoversAfterPhoneRevisionResets() {
        let staleWatchCache = settings(
            revision: 6,
            updatedAt: Date(timeIntervalSince1970: 1_000)
        )
        let reinstalledPhone = settings(
            revision: 1,
            updatedAt: Date(timeIntervalSince1970: 2_000)
        )

        XCTAssertTrue(WatchScheduleStore.shouldApply(reinstalledPhone, over: staleWatchCache))
        XCTAssertFalse(WatchScheduleStore.shouldApply(staleWatchCache, over: reinstalledPhone))
    }

    func testContractRejectsInvalidTimezoneAndNumericValues() async throws {
        var invalid = settings(revision: 2)
        XCTAssertTrue(invalid.isValid)
        invalid = WatchSettingsEnvelope(
            schemaVersion: 1, revision: 3, timezone: "", workIntervalSeconds: 1200,
            shortBreakSeconds: 20, longBreakSeconds: 300, preBreakSeconds: 30,
            activeDaysMask: 127, activeStartMinutes: 0, activeEndMinutes: 0, paused: false,
            updatedAt: .now, nextBreakAt: nil, breakActive: false, breakKind: nil, phase: nil, phaseDeadlineAt: nil
        )
        XCTAssertFalse(invalid.isValid)
        let store = WatchScheduleStore()
        let applied = await store.apply(invalid)
        XCTAssertFalse(applied)
    }

    func testSharedFixtureDecodesAndFutureFieldsAreIgnored() throws {
        let fixture = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("tests/fixtures/watch-settings-v1.json")
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let decoded = try decoder.decode(WatchSettingsEnvelope.self, from: Data(contentsOf: fixture))
        XCTAssertEqual(decoded.schemaVersion, 1)
        XCTAssertEqual(decoded.revision, 7)
        XCTAssertEqual(decoded.timezone, "Europe/Berlin")
        XCTAssertEqual(decoded.nextBreakAt, Date(timeIntervalSince1970: 1_784_794_800))
        XCTAssertNil(decoded.phase)
        XCTAssertNil(decoded.phaseDeadlineAt)
    }

    func testOfflineReminderPlanUsesActiveHoursAndStaysBounded() throws {
        let fixture = URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
            .deletingLastPathComponent().deletingLastPathComponent()
            .appendingPathComponent("tests/fixtures/watch-settings-v1.json")
        let data = try Data(contentsOf: fixture)
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let settings = try decoder.decode(WatchSettingsEnvelope.self, from: data)
        let now = ISO8601DateFormatter().date(from: "2026-07-27T08:50:00Z")!
        let dates = WatchScheduleStore.reminderDates(
            for: settings,
            firstBreakAt: ISO8601DateFormatter().date(from: "2026-07-27T09:10:00Z"),
            now: now,
            limit: 3
        )
        XCTAssertEqual(dates.count, 3)
        XCTAssertTrue(dates.allSatisfy { $0 >= now })
        XCTAssertEqual(WatchScheduleStore.reminderDates(for: settings, firstBreakAt: nil, now: now, limit: 0), [])
    }

    func testCountdownRingDrainsWithRemainingLocalTime() {
        let start = Date(timeIntervalSince1970: 1_000)
        let halfway = start.addingTimeInterval(600)
        XCTAssertEqual(WatchScheduleStore.remainingSeconds(startedAt: start, durationSeconds: 1_200, now: halfway), 600)
        XCTAssertEqual(WatchScheduleStore.remainingFraction(startedAt: start, durationSeconds: 1_200, now: start), 1)
        XCTAssertEqual(WatchScheduleStore.remainingFraction(startedAt: start, durationSeconds: 1_200, now: halfway), 0.5)
        XCTAssertEqual(WatchScheduleStore.remainingFraction(startedAt: start, durationSeconds: 1_200, now: start.addingTimeInterval(1_500)), 0)
    }

    func testCountdownClampsBeforeStartAndAfterDeadline() {
        let start = Date(timeIntervalSince1970: 1_000)
        XCTAssertEqual(
            WatchScheduleStore.remainingSeconds(
                startedAt: start,
                durationSeconds: 20,
                now: start.addingTimeInterval(-5)
            ),
            20
        )
        XCTAssertEqual(
            WatchScheduleStore.remainingSeconds(
                startedAt: start,
                durationSeconds: 20,
                now: start.addingTimeInterval(25)
            ),
            0
        )
        XCTAssertEqual(
            WatchScheduleStore.remainingFraction(
                startedAt: start,
                durationSeconds: 20,
                now: start.addingTimeInterval(-5)
            ),
            1
        )
    }

    func testPhasePayloadDecodesWithoutBreakingOlderEnvelopeSupport() throws {
        let data = Data("""
        {
          "schema_version": 1,
          "revision": 8,
          "timezone": "UTC",
          "work_interval_seconds": 1200,
          "short_break_seconds": 20,
          "long_break_seconds": 300,
          "pre_break_seconds": 30,
          "active_days_mask": 127,
          "active_start_minutes": 0,
          "active_end_minutes": 0,
          "paused": false,
          "updated_at": "2026-07-23T08:00:00Z",
          "phase": { "breaking": { "kind": "long" } },
          "phase_deadline_at": "2026-07-23T08:05:00Z"
        }
        """.utf8)
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let decoded = try decoder.decode(WatchSettingsEnvelope.self, from: data)

        XCTAssertEqual(decoded.phase, .breaking(kind: "long"))
        XCTAssertEqual(decoded.phaseDeadlineAt, Date(timeIntervalSince1970: 1_784_793_900))
    }

    func testFlatBreakingPhaseUsesEnvelopeBreakKindForDuration() throws {
        let data = Data("""
        {
          "schema_version": 1,
          "revision": 9,
          "timezone": "UTC",
          "work_interval_seconds": 1200,
          "short_break_seconds": 20,
          "long_break_seconds": 300,
          "pre_break_seconds": 30,
          "active_days_mask": 127,
          "active_start_minutes": 0,
          "active_end_minutes": 0,
          "paused": false,
          "updated_at": "2026-07-23T08:00:00Z",
          "break_kind": "long",
          "phase": "breaking"
        }
        """.utf8)
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let decoded = try decoder.decode(WatchSettingsEnvelope.self, from: data)

        XCTAssertEqual(decoded.phase, .breaking(kind: nil))
        XCTAssertEqual(decoded.phase?.duration(in: decoded), 300)
    }

    func testActiveHoursMatchAllDayAndOvernightEngineRules() {
        var utc = Calendar(identifier: .gregorian)
        utc.timeZone = TimeZone(identifier: "UTC")!
        let allDay = WatchSettingsEnvelope.localDefaults(
            now: Date(timeIntervalSince1970: 0),
            timeZone: utc.timeZone
        )
        XCTAssertTrue(WatchScheduleStore.isActive(
            allDay,
            at: ISO8601DateFormatter().date(from: "2026-08-02T12:00:00Z")!,
            calendar: utc
        ))
        XCTAssertEqual(
            WatchScheduleStore.reminderDates(
                for: allDay,
                firstBreakAt: Date(timeIntervalSince1970: 1_200),
                now: Date(timeIntervalSince1970: 0),
                limit: 1
            ),
            [Date(timeIntervalSince1970: 1_200)]
        )

        let overnight = settings(
            revision: 1,
            activeDaysMask: 1,
            activeStartMinutes: 22 * 60,
            activeEndMinutes: 6 * 60
        )
        XCTAssertTrue(WatchScheduleStore.isActive(
            overnight,
            at: ISO8601DateFormatter().date(from: "2026-08-02T23:00:00Z")!,
            calendar: utc
        ))
        XCTAssertTrue(WatchScheduleStore.isActive(
            overnight,
            at: ISO8601DateFormatter().date(from: "2026-08-02T03:00:00Z")!,
            calendar: utc
        ))
        XCTAssertFalse(WatchScheduleStore.isActive(
            overnight,
            at: ISO8601DateFormatter().date(from: "2026-08-03T03:00:00Z")!,
            calendar: utc
        ))
    }

    private func settings(
        revision: UInt64,
        activeDaysMask: Int = 62,
        activeStartMinutes: Int = 540,
        activeEndMinutes: Int = 1_080,
        updatedAt: Date = .now
    ) -> WatchSettingsEnvelope {
        WatchSettingsEnvelope(
            schemaVersion: 1,
            revision: revision,
            timezone: "UTC",
            workIntervalSeconds: 1_200,
            shortBreakSeconds: 20,
            longBreakSeconds: 300,
            preBreakSeconds: 30,
            activeDaysMask: activeDaysMask,
            activeStartMinutes: activeStartMinutes,
            activeEndMinutes: activeEndMinutes,
            paused: false,
            updatedAt: updatedAt,
            nextBreakAt: nil,
            breakActive: false,
            breakKind: nil,
            phase: nil,
            phaseDeadlineAt: nil
        )
    }
}
