/** Shared timed-pause options offered by both the break-due prompt and the dashboard. */
export const pauseChoices = [
  { minutes: 30, labelKey: 'break_pause_30' },
  { minutes: 60, labelKey: 'break_pause_60' },
  { minutes: 120, labelKey: 'break_pause_120' },
] as const
