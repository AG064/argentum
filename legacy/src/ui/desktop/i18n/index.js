/**
 * Argentum i18n source module. The desktop-safe JavaScript copy is generated
 * by scripts/sync-desktop-assets.js so locale data is included in Tauri's frontendDist.
 *
 * Design principles:
 * - en is the authoritative source. All keys must exist in en.
 * - Other locales (et, etc.) are partial overrides.
 * - Missing keys fall back to the English value.
 * - ICU-style {placeholder} syntax is supported in values.
 * - The active locale is stored in state.uiLanguage and persisted via UI preferences.
 *
 * Usage in frontend JS:
 *   import { t, setLocale, getLocale, formatNumber, formatDate, SUPPORTED_LOCALES }
 *     from './i18n/index.js';
 *   t('chat.welcome')                           // "Welcome to Argentum"
 *   t('errors.notFound', { id: 'widget' })     // "Widget 'widget' not found"
 *   formatNumber(1234567, 'de-DE')              // "1.234.567"
 *   formatDate(new Date(), 'short')             // locale-aware date string
 */

import en from './en.js';
import et from './et.js';

// ─── Registry ────────────────────────────────────────────────────────────────

/** @type {readonly {code: string, label: string, dir: 'ltr'|'rtl'}[]} */
export const SUPPORTED_LOCALES = Object.freeze([
  Object.freeze({ code: 'en', label: 'English', dir: 'ltr' }),
  Object.freeze({ code: 'et', label: 'Eesti', dir: 'ltr' }),
]);

const REGISTRY = Object.freeze({ en, et });

// ─── State ───────────────────────────────────────────────────────────────────

/** @type {string} */
let currentLocale = 'en';

/** @type {Set<(locale: string) => void>} */
const changeListeners = new Set();

// ─── Core API ────────────────────────────────────────────────────────────────

/** Get the currently active locale code. */
export function getLocale() {
  return currentLocale;
}

/**
 * Set the active locale and notify all listeners.
 * @param {string} locale
 */
export function setLocale(locale) {
  if (!REGISTRY[locale]) {
    console.warn(`[i18n] Unknown locale "${locale}", falling back to "en"`);
    locale = 'en';
  }
  if (locale === currentLocale) return;
  currentLocale = locale;
  for (const listener of changeListeners) {
    try {
      listener(locale);
    } catch (err) {
      console.error('[i18n] Listener error', err);
    }
  }
  document.dispatchEvent(new CustomEvent('localechange', { detail: { locale } }));
}

/**
 * Register a listener called whenever the locale changes.
 * Returns an unsubscribe function.
 * @param {(locale: string) => void} listener
 * @returns {() => void}
 */
export function onLocaleChange(listener) {
  changeListeners.add(listener);
  return () => changeListeners.delete(listener);
}

/**
 * Resolve a dot-notation key from an object.
 * @param {unknown} obj
 * @param {string[]} parts
 * @returns {string|undefined}
 */
function resolveKey(obj, parts) {
  let current = obj;
  for (const part of parts) {
    if (current === null || current === undefined || typeof current !== 'object') return undefined;
    current = current[part];
  }
  return typeof current === 'string' ? current : undefined;
}

/**
 * Translate a key. Missing keys fall back to the English value.
 *
 * @param {string} key       Dot-notation key, e.g. 'nav.settings' or 'errors.notFound'
 * @param {Record<string, string|number>} [params]  Optional placeholder values, e.g. { name: 'Alice' }
 *
 * @example
 *   t('app.name')                              // "Argentum"
 *   t('errors.notFound', { id: '123' })        // "Record '123' not found"
 */
export function t(key, params) {
  const parts = key.split('.');
  // Try active locale
  let value = resolveKey(REGISTRY[currentLocale], parts);
  if (value !== undefined) return interpolate(String(value), params);

  // Fall back to English
  value = resolveKey(REGISTRY['en'], parts);
  if (value !== undefined) {
    if (currentLocale !== 'en') {
      console.warn(`[i18n] Missing key "${key}" in locale "${currentLocale}"`);
    }
    return interpolate(String(value), params);
  }

  console.warn(`[i18n] Missing key "${key}" in all locales`);
  return key;
}

/**
 * Replace {placeholder} tokens in a string.
 * @param {string} str
 * @param {Record<string, string|number>|undefined} params
 * @returns {string}
 */
function interpolate(str, params) {
  if (!params) return str;
  return str.replace(/\{(\w+)\}/g, (_, k) => {
    return Object.prototype.hasOwnProperty.call(params, k) ? String(params[k]) : `{${k}}`;
  });
}

// ─── Intl formatters ─────────────────────────────────────────────────────────

/**
 * Format a number using Intl.NumberFormat.
 * @param {number} value
 * @param {Intl.NumberFormatOptions} [options]
 * @param {string} [localeCode]
 */
export function formatNumber(value, options, localeCode) {
  const locale = localeCode ?? currentLocale;
  return new Intl.NumberFormat(locale, options).format(value);
}

/**
 * Format a date using Intl.DateTimeFormat.
 * @param {Date|number|string} value
 * @param {'short'|'medium'|'long'|'full'} [preset]
 * @param {string} [localeCode]
 */
export function formatDate(value, preset = 'medium', localeCode) {
  const date = value instanceof Date ? value : new Date(value);
  return new Intl.DateTimeFormat(localeCode ?? currentLocale, {
    dateStyle: preset,
  }).format(date);
}

/**
 * Format a relative time string (e.g. "2 minutes ago").
 * @param {Date|number} value
 * @param {Intl.RelativeTimeFormatUnit} [unit]
 * @param {string} [localeCode]
 */
export function formatRelativeTime(value, unit = 'second', localeCode) {
  const date = value instanceof Date ? value : new Date(value);
  const now = Date.now();
  const diffMs = date.getTime() - now;
  const diffSec = diffMs / 1000;
  const diffMin = diffSec / 60;
  const diffHour = diffMin / 60;
  const diffDay = diffHour / 24;

  const unitMap = {
    second: diffSec,
    minute: diffMin,
    hour: diffHour,
    day: diffDay,
    week: diffDay / 7,
    month: diffDay / 30,
    quarter: diffDay / 90,
    year: diffDay / 365,
  };

  const rtf = new Intl.RelativeTimeFormat(localeCode ?? currentLocale, { numeric: 'auto' });
  return rtf.format(Math.round(unitMap[unit] || diffSec), unit);
}

// ─── Text direction ──────────────────────────────────────────────────────────

/** Returns 'rtl' for RTL locales, 'ltr' otherwise. */
export function textDirection(localeCode) {
  const locale = localeCode ?? currentLocale;
  const entry = SUPPORTED_LOCALES.find((l) => l.code === locale);
  return entry?.dir ?? 'ltr';
}

/** Returns the BCP-47 language tag for a locale code. */
export function bcp47(localeCode) {
  const map = { en: 'en-US', et: 'et-EE' };
  return map[localeCode ?? currentLocale] ?? 'en-US';
}

/** Returns the display label for a locale code. */
export function localeLabel(localeCode) {
  const entry = SUPPORTED_LOCALES.find((l) => l.code === localeCode);
  return entry?.label ?? localeCode;
}
