//! Offline reminder planning for hosts that cannot keep a timer running.
//!
//! A desktop process ticks once a second for as long as it is open, so it can
//! decide at the moment a break falls due. A phone cannot: iOS suspends the
//! app and Android dozes it, and neither guarantees any code runs at the
//! instant a break is due. The only mechanism that survives that is a
//! notification registered *in advance* for an absolute instant.
//!
//! So the phone pre-computes the next several break instants here and hands
//! them to the platform scheduler. This mirrors what both watch companions
//! already do (`WatchScheduleStore.reminderDates`, `PausIOWearReminderPlanner`)
//! and keeps the schedule rules — the day mask and the overnight active window
//! — in exactly one tested place rather than reimplemented per platform.
//!
//! A plan contains instants and nothing else. It never carries activity,
//! application names, or any content, and it is never transmitted anywhere.

use chrono::{DateTime, Duration, Local, TimeZone, Utc};
use pausio_protocol::{ReminderKind, ReminderSlot, TimerPhase};

use crate::settings::Settings;

/// Bounds the search for active-window slots so a schedule that is active for
/// only a few minutes a week cannot spin. Mirrors the Swift companion's
/// `limit * 32` guard.
const MAX_CANDIDATE_STEPS_PER_SLOT: usize = 32;

/// Builds the next reminder instants from the current phase and settings.
///
/// `phase_deadline` is when the *current* phase ends, which anchors the first
/// break; without one the first break is a full work interval away. Returns an
/// empty plan whenever nothing should fire — paused, dormant, or a zero limit —
/// which is what a caller registers to cancel everything.
pub fn reminder_plan(
    settings: &Settings,
    phase: &TimerPhase,
    phase_deadline: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    limit: usize,
) -> Vec<ReminderSlot> {
    if limit == 0 || settings.work_seconds == 0 {
        return vec![];
    }
    // A paused or dormant timer has no next break. Returning an empty plan is
    // meaningful: callers register it, which clears anything still pending.
    match phase {
        TimerPhase::Paused { .. } | TimerPhase::Dormant => return vec![],
        _ => {}
    }

    let interval = Duration::seconds(i64::from(settings.work_seconds));

    // A break that is already due should not be re-announced by the scheduler;
    // the next thing worth a reminder is the break after this one.
    let first = match phase {
        TimerPhase::Breaking { .. } => phase_deadline
            .map(|deadline| deadline + interval)
            .unwrap_or(now + interval),
        TimerPhase::BreakDue { .. } => now + interval,
        _ => phase_deadline.unwrap_or(now + interval),
    };
    // Never schedule in the past: a stale deadline would otherwise fire
    // immediately on every refresh.
    let mut candidate = first.max(now + Duration::seconds(1));

    let mut slots = Vec::with_capacity(limit.min(64));
    let max_steps = limit.saturating_mul(MAX_CANDIDATE_STEPS_PER_SLOT);
    let pre_break = i64::from(settings.pre_break_seconds);

    for _ in 0..max_steps {
        if slots.len() >= limit {
            break;
        }
        if is_active_at(settings, candidate) {
            if pre_break > 0 {
                let pre_at = candidate - Duration::seconds(pre_break);
                // Only warn if the warning itself is still ahead of us;
                // a pre-break cue fired at the same moment as the break is noise.
                if pre_at > now && slots.len() < limit {
                    slots.push(ReminderSlot {
                        at: pre_at,
                        kind: ReminderKind::PreBreak,
                    });
                }
            }
            if slots.len() < limit {
                slots.push(ReminderSlot {
                    at: candidate,
                    kind: ReminderKind::BreakDue,
                });
            }
        }
        candidate += interval;
    }
    slots
}

/// Evaluates the configured active window against an instant, in local time.
///
/// `Settings::active_at` already owns the day-mask and overnight-range rules;
/// this only moves a UTC instant into the local zone so those rules stay in a
/// single place.
fn is_active_at(settings: &Settings, at: DateTime<Utc>) -> bool {
    let local = Local.from_utc_datetime(&at.naive_utc());
    settings.active_at(local)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;
    use pausio_protocol::BreakKind;

    fn always_on() -> Settings {
        Settings {
            // Every day, all hours: isolates the stepping logic from the
            // active-window logic, which has dedicated tests below.
            active_days_mask: 0b0111_1111,
            active_start_minutes: 0,
            active_end_minutes: 0,
            work_seconds: 1200,
            pre_break_seconds: 0,
            ..Settings::default()
        }
    }

    #[test]
    fn a_paused_timer_has_no_plan() {
        let plan = reminder_plan(
            &always_on(),
            &TimerPhase::Paused {
                reason: pausio_protocol::PauseReason::Manual,
            },
            Some(Utc::now() + Duration::seconds(60)),
            Utc::now(),
            16,
        );
        assert!(plan.is_empty());
    }

    #[test]
    fn a_dormant_timer_has_no_plan() {
        let plan = reminder_plan(&always_on(), &TimerPhase::Dormant, None, Utc::now(), 16);
        assert!(plan.is_empty());
    }

    #[test]
    fn a_zero_limit_produces_no_plan() {
        let plan = reminder_plan(&always_on(), &TimerPhase::Working, None, Utc::now(), 0);
        assert!(plan.is_empty());
    }

    #[test]
    fn the_first_slot_anchors_on_the_current_deadline() {
        let now = Utc::now();
        let deadline = now + Duration::seconds(300);
        let plan = reminder_plan(&always_on(), &TimerPhase::Working, Some(deadline), now, 3);
        assert_eq!(plan[0].at, deadline);
        assert_eq!(plan[0].kind, ReminderKind::BreakDue);
    }

    #[test]
    fn without_a_deadline_the_first_slot_is_one_interval_away() {
        let now = Utc::now();
        let plan = reminder_plan(&always_on(), &TimerPhase::Working, None, now, 2);
        assert_eq!(plan[0].at, now + Duration::seconds(1200));
    }

    #[test]
    fn slots_step_by_the_work_interval_and_stay_ordered() {
        let now = Utc::now();
        let plan = reminder_plan(
            &always_on(),
            &TimerPhase::Working,
            Some(now + Duration::seconds(60)),
            now,
            5,
        );
        assert_eq!(plan.len(), 5);
        for pair in plan.windows(2) {
            assert!(pair[1].at > pair[0].at, "plan must be strictly increasing");
            assert_eq!(pair[1].at - pair[0].at, Duration::seconds(1200));
        }
    }

    #[test]
    fn a_plan_is_bounded_by_its_limit() {
        let now = Utc::now();
        let plan = reminder_plan(&always_on(), &TimerPhase::Working, None, now, 7);
        assert_eq!(plan.len(), 7);
    }

    #[test]
    fn a_stale_deadline_never_schedules_in_the_past() {
        let now = Utc::now();
        let plan = reminder_plan(
            &always_on(),
            &TimerPhase::Working,
            Some(now - Duration::seconds(5_000)),
            now,
            4,
        );
        assert!(
            plan.iter().all(|slot| slot.at > now),
            "a past deadline must not fire immediately"
        );
    }

    #[test]
    fn pre_break_slots_are_emitted_when_configured() {
        let settings = Settings {
            pre_break_seconds: 30,
            ..always_on()
        };
        let now = Utc::now();
        let deadline = now + Duration::seconds(600);
        let plan = reminder_plan(&settings, &TimerPhase::Working, Some(deadline), now, 4);
        assert_eq!(plan[0].kind, ReminderKind::PreBreak);
        assert_eq!(plan[0].at, deadline - Duration::seconds(30));
        assert_eq!(plan[1].kind, ReminderKind::BreakDue);
        assert_eq!(plan[1].at, deadline);
    }

    #[test]
    fn no_pre_break_slots_when_the_warning_is_disabled() {
        let now = Utc::now();
        let plan = reminder_plan(&always_on(), &TimerPhase::Working, None, now, 6);
        assert!(plan.iter().all(|slot| slot.kind == ReminderKind::BreakDue));
    }

    #[test]
    fn a_pre_break_already_in_the_past_is_skipped_but_the_break_is_kept() {
        let settings = Settings {
            pre_break_seconds: 60,
            ..always_on()
        };
        let now = Utc::now();
        // The break is 10s away, so its 60s warning is already behind us.
        let deadline = now + Duration::seconds(10);
        let plan = reminder_plan(&settings, &TimerPhase::Working, Some(deadline), now, 3);
        assert_eq!(plan[0].kind, ReminderKind::BreakDue);
        assert_eq!(plan[0].at, deadline);
    }

    #[test]
    fn an_active_break_schedules_the_following_interval_not_this_one() {
        let now = Utc::now();
        let break_ends = now + Duration::seconds(20);
        let plan = reminder_plan(
            &always_on(),
            &TimerPhase::Breaking {
                kind: BreakKind::Short,
            },
            Some(break_ends),
            now,
            2,
        );
        // The break already has the person's attention; the next reminder is
        // one full work interval after it ends.
        assert_eq!(plan[0].at, break_ends + Duration::seconds(1200));
    }

    #[test]
    fn an_already_due_break_is_not_re_announced() {
        let now = Utc::now();
        let plan = reminder_plan(
            &always_on(),
            &TimerPhase::BreakDue {
                kind: BreakKind::Short,
            },
            None,
            now,
            2,
        );
        assert_eq!(plan[0].at, now + Duration::seconds(1200));
    }

    #[test]
    fn slots_outside_the_active_window_are_skipped() {
        // A one-hour daily window. With a 20-minute interval only a handful of
        // instants per day qualify, so the planner must step over the rest
        // instead of emitting them.
        let settings = Settings {
            active_days_mask: 0b0111_1111,
            active_start_minutes: 9 * 60,
            active_end_minutes: 10 * 60,
            work_seconds: 1200,
            pre_break_seconds: 0,
            ..Settings::default()
        };
        let now = Utc::now();
        let plan = reminder_plan(&settings, &TimerPhase::Working, None, now, 6);
        for slot in &plan {
            let local = Local.from_utc_datetime(&slot.at.naive_utc());
            assert!(
                settings.active_at(local),
                "planner emitted a slot outside the active window: {local}"
            );
        }
    }

    #[test]
    fn an_overnight_window_is_honoured() {
        // 22:00 -> 02:00 crosses midnight; active_at owns that rule and the
        // planner must not contradict it.
        let settings = Settings {
            active_days_mask: 0b0111_1111,
            active_start_minutes: 22 * 60,
            active_end_minutes: 2 * 60,
            work_seconds: 1200,
            pre_break_seconds: 0,
            ..Settings::default()
        };
        let now = Utc::now();
        let plan = reminder_plan(&settings, &TimerPhase::Working, None, now, 5);
        assert!(!plan.is_empty(), "an overnight window still has slots");
        for slot in &plan {
            let local = Local.from_utc_datetime(&slot.at.naive_utc());
            assert!(settings.active_at(local));
        }
    }

    #[test]
    fn a_single_active_day_still_terminates_and_stays_on_that_day() {
        // Sunday only. This is the pathological case for the step cap: most
        // candidates are rejected, so the guard must stop the search rather
        // than let it run unbounded.
        let settings = Settings {
            active_days_mask: 0b0000_0001,
            active_start_minutes: 0,
            active_end_minutes: 0,
            work_seconds: 1200,
            pre_break_seconds: 0,
            ..Settings::default()
        };
        let plan = reminder_plan(&settings, &TimerPhase::Working, None, Utc::now(), 4);
        for slot in &plan {
            let local = Local.from_utc_datetime(&slot.at.naive_utc());
            assert!(settings.active_at(local));
        }
    }
}
