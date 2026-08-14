// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2026 AG064
// This file intentionally has no imports. It must be able to report import and
// initialization failures from the rest of the desktop UI.
(function installStartupDiagnostics() {
  'use strict';

  const MAX_DETAIL_LENGTH = 12000;
  const CURRENT_VERSION = '0.0.9';
  const RELEASES_URL = 'https://github.com/AG064/argentum/releases/latest';
  const REPORT_EMAIL = 'report@ag064.eu';
  let reported = false;

  function text(value) {
    if (value instanceof Error) return value.stack || `${value.name}: ${value.message}`;
    if (typeof value === 'string') return value;
    try {
      return JSON.stringify(value);
    } catch {
      return String(value);
    }
  }

  function versionParts(value) {
    const match = String(value || '')
      .trim()
      .replace(/^v/i, '')
      .match(/^(\d+)(?:\.(\d+))?(?:\.(\d+))?/);
    return match ? match.slice(1).map((part) => Number(part || 0)) : null;
  }

  function isNewerVersion(candidate, current) {
    const left = versionParts(candidate);
    const right = versionParts(current);
    if (!left || !right) return false;
    for (let index = 0; index < 3; index += 1) {
      if (left[index] !== right[index]) return left[index] > right[index];
    }
    return false;
  }

  async function checkRecoveryUpdate() {
    const invoke = window.__TAURI__?.core?.invoke;
    if (typeof invoke !== 'function') {
      throw new Error('Desktop update bridge unavailable');
    }

    const result = await invoke('check_for_updates');
    const version = String(result?.version || '')
      .trim()
      .replace(/^v/i, '');
    const url = String(result?.releaseUrl || result?.release_url || '').trim();
    if (!/^https:\/\/github\.com\/AG064\/argentum\/releases\//.test(url)) {
      throw new Error('GitHub returned an unexpected release URL');
    }

    return {
      available:
        Boolean(result?.updateAvailable ?? result?.update_available) &&
        isNewerVersion(version, CURRENT_VERSION),
      version,
      url,
    };
  }

  async function openExternalTarget(target) {
    const invoke = window.__TAURI__?.core?.invoke;
    if (invoke) {
      try {
        await invoke('open_external_url', { request: { url: target } });
        return;
      } catch {
        // Fall back to the webview browser behavior if native browser launch fails.
      }
    }
    window.open(target, '_blank', 'noopener,noreferrer');
  }

  async function openRecoveryUpdate(url) {
    await openExternalTarget(url || RELEASES_URL);
  }

  function showFailure(reason, source) {
    if (reported) return;
    reported = true;
    const detail = text(reason).slice(0, MAX_DETAIL_LENGTH) || 'No error details were provided.';
    const root = document.querySelector('#view-root');
    if (!root) return;

    root.innerHTML = `
      <section class="startup-failure" role="alert" aria-live="assertive">
        <p class="eyebrow">STARTUP ERROR</p>
        <h2>Argentum could not finish loading</h2>
        <p>The desktop shell is open, but its interface failed during ${source}.</p>
        <p class="startup-failure-hint">Use the details below when checking the app log or reporting the problem.</p>
        <pre class="startup-failure-details"></pre>
        <div class="startup-failure-actions">
          <button class="button primary" type="button" data-startup-reload>Reload Argentum</button>
          <button class="button" type="button" data-startup-copy>Copy details</button>
          <button class="button" type="button" data-startup-report>Report on GitHub</button>
          <button class="button" type="button" data-startup-email>Email report</button>
          <button class="button" type="button" data-startup-update hidden>Check for updates</button>
        </div>
        <p class="startup-failure-status" aria-live="polite"></p>
      </section>`;

    const details = root.querySelector('.startup-failure-details');
    if (details) details.textContent = `${source}:\n${detail}`;
    root
      .querySelector('[data-startup-reload]')
      ?.addEventListener('click', () => window.location.reload());
    root.querySelector('[data-startup-copy]')?.addEventListener('click', async () => {
      const status = root.querySelector('.startup-failure-status');
      try {
        await navigator.clipboard.writeText(`${source}:\n${detail}`);
        if (status) status.textContent = 'Details copied.';
      } catch {
        if (status) status.textContent = 'Copy was unavailable. Select the details manually.';
      }
    });
    root.querySelector('[data-startup-report]')?.addEventListener('click', async () => {
      const version =
        document.querySelector('.brand-wordmark span')?.textContent?.trim() || 'unknown';
      const body = [
        '## Startup error',
        '',
        `Argentum could not finish loading during ${source}.`,
        '',
        '## Error details',
        '```text',
        detail,
        '```',
        '',
        '## Environment',
        `- Argentum version: ${version}`,
        `- Platform: ${navigator.platform || 'unknown'}`,
        `- User agent: ${navigator.userAgent || 'unknown'}`,
        '',
        '## Steps to reproduce',
        '1. Start Argentum.',
        '2. Observe the startup error.',
      ].join('\n');
      const url =
        'https://github.com/AG064/argentum/issues/new?title=' +
        encodeURIComponent('[Startup error] Argentum could not finish loading') +
        '&labels=bug&body=' +
        encodeURIComponent(body);
      const invoke = window.__TAURI__?.core?.invoke;
      if (invoke) {
        try {
          await invoke('open_external_url', { request: { url } });
          return;
        } catch {
          // Fall back to the webview browser behavior if native browser launch fails.
        }
      }
      await openExternalTarget(url);
    });
    root.querySelector('[data-startup-email]')?.addEventListener('click', async () => {
      const emailBody = [
        'Argentum startup error',
        '',
        `Phase: ${source}`,
        `Version: ${CURRENT_VERSION}`,
        `Platform: ${navigator.platform || 'unknown'}`,
        `User agent: ${navigator.userAgent || 'unknown'}`,
        '',
        'Error details:',
        detail,
      ].join('\n');
      await openExternalTarget(
        `mailto:${REPORT_EMAIL}?subject=${encodeURIComponent('[Argentum] Startup error')}&body=${encodeURIComponent(emailBody)}`,
      );
    });

    const updateButton = root.querySelector('[data-startup-update]');
    const status = root.querySelector('.startup-failure-status');
    checkRecoveryUpdate()
      .then((update) => {
        if (!update.available) {
          if (status) status.textContent = 'Recovery update check: no newer release found.';
          return;
        }
        if (!(updateButton instanceof HTMLButtonElement)) return;
        updateButton.hidden = false;
        updateButton.textContent = `Update to Argentum ${update.version}`;
        updateButton.addEventListener('click', () => openRecoveryUpdate(update.url));
        if (status)
          status.textContent = `A newer release is available: Argentum ${update.version}.`;
      })
      .catch((error) => {
        if (status) status.textContent = `Recovery update check unavailable: ${text(error)}`;
      });
  }

  window.addEventListener('error', (event) => {
    showFailure(event.error || event.message || 'Unknown JavaScript error', 'renderer startup');
  });
  window.addEventListener('unhandledrejection', (event) => {
    showFailure(event.reason || 'Unhandled promise rejection', 'startup promise');
  });

  window.__ARGENTUM_STARTUP__ = Object.freeze({
    reportFailure: showFailure,
  });
})();
