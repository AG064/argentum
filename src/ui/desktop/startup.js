// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2026 AG064
// This file intentionally has no imports. It must be able to report import and
// initialization failures from the rest of the desktop UI.
(function installStartupDiagnostics() {
  'use strict';

  const MAX_DETAIL_LENGTH = 12000;
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
      window.open(url, '_blank', 'noopener,noreferrer');
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
