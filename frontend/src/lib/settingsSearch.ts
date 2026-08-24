import { t } from './i18n'
import { SETTINGS_INDEX, type SettingsIndexEntry } from './settingsIndex'

export interface SettingsSearchResult {
  entry: SettingsIndexEntry
  label: string
}

/** Case-insensitive substring match against each entry's translated label and hint. */
export function searchSettings(query: string): SettingsSearchResult[] {
  const needle = query.trim().toLowerCase()
  if (!needle) return []
  return SETTINGS_INDEX.filter((entry) => {
    const label = t(entry.labelKey)
    const hint = entry.hintKey ? t(entry.hintKey) : ''
    return label.toLowerCase().includes(needle) || hint.toLowerCase().includes(needle)
  }).map((entry) => ({ entry, label: t(entry.labelKey) }))
}
