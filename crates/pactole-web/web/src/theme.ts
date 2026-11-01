/** Theme names supported by the interface. */
export type ThemeName = 'light' | 'dark'

const STORAGE_KEY = 'pactole.theme'

/** Returns whether a value is a supported theme name. */
export function isThemeName(value: unknown): value is ThemeName {
  return value === 'light' || value === 'dark'
}

/** Resolves the initial theme from storage, then the system preference. */
export function detectTheme(
  storage?: { getItem(key: string): string | null } | null,
  prefersDark?: boolean,
): ThemeName {
  try {
    const stored = storage?.getItem(STORAGE_KEY)
    if (isThemeName(stored)) {
      return stored
    }
  } catch {
    // Storage can be unavailable (private mode, disabled cookies).
  }
  return prefersDark ? 'dark' : 'light'
}

function defaultStorage(): { getItem(key: string): string | null; setItem?(key: string, value: string): void } | null {
  try {
    return typeof localStorage === 'undefined' ? null : localStorage
  } catch {
    return null
  }
}

function systemPrefersDark(): boolean {
  return typeof matchMedia !== 'undefined' && matchMedia('(prefers-color-scheme: dark)').matches
}

/** Persists the selected theme. */
export function storeTheme(theme: ThemeName): void {
  try {
    defaultStorage()?.setItem?.(STORAGE_KEY, theme)
  } catch {
    // Persisting the preference is best-effort.
  }
}

/** Returns the initial theme of the application. */
export function initialTheme(): ThemeName {
  return detectTheme(defaultStorage(), systemPrefersDark())
}
