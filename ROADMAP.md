# Roadmap

## Current Status

Argentum v0.0.7 is released and functional. The project is actively maintained.

## What We Will Do

### v0.1.0 (Next 3-6 months)

- **Internationalization (i18n) infrastructure** — enable localization for any language
  - Extract all user-facing strings to locale files (en.json as base)
  - Implement t('key') translation function used throughout codebase
  - Support text direction (LTR/RTL) via CSS logical properties
  - Number/date formatting via Intl API (not hardcoded formats)
  - Language picker in settings and onboarding
- **Russian language UI and documentation**
- **Estonian language UI and documentation**
- Test suite fixes (Jest/ESM configuration)
- Improved test coverage (expand to more code branches)

### Accessibility Improvements (v0.1.0)

- **Keyboard navigation** — shortcuts and tabindex throughout the app
  - All interactive elements reachable via Tab
  - Visible focus indicators
  - Shortcut keys for common actions (send message, open settings, navigate channels)
- **Screen reader support** — ARIA labels and roles where needed
  - Test with at least one screen reader on each target platform
  - Add aria-label to icon-only buttons
  - Ensure progress indicators and status elements are announced
- **Visual accessibility settings** — user-configurable in settings and onboarding
  - Text contrast adjustment (high contrast mode)
  - Accent color picker (replace default red with any color; logo stays unchanged)
  - Error/warning text styling independent of color (icons + text, not color alone)
- **In-app update mechanism** — check for and apply updates without manual reinstall
- **Interactive help button (?)** — context-sensitive help tooltip linked to relevant docs section; appears on hover/focus near each major UI element
  - Clicking ? opens relevant documentation page in browser
  - Tooltip text explains what the element does and how to use it
  - ? button visible in onboarding flow and settings
- **Accessibility documentation updates** — new menus and features include keyboard shortcuts and ARIA attributes in each release

### Future (6-12 months)

- Tauri v3.0 upgrade when stable (resolves glib vulnerability)
- Additional messaging channels as demand warrants
- Performance improvements and memory optimization

## What We Will NOT Do

- Cloud hosting or managed service (project remains self-hosted only)
- Collecting user data or analytics
- Mobile-native app (desktop focus only)
- Windows/Mac exclusive features (cross-platform remains a goal)

## How Roadmap Is Updated

The roadmap is reviewed monthly and updated as priorities shift. Major changes are discussed in GitHub issues. New features must include keyboard navigation and ARIA attributes as part of implementation, not as an afterthought.

## v0.1.0 Notes

Feedback mechanism in-code: users should be able to submit bug reports and feature requests from within the running application, not just through GitHub.

Russian language: UI strings, onboarding, and docs to support the Russian-speaking user base.

Accent color: the default red accent (#e23b3b) should be user-configurable in settings. Users who need different colors for visual comfort or branding can choose their own. The Argentum logo itself does not change.

Accessibility as process: every new UI element added after v0.1.0 must ship with keyboard navigation (tabindex if interactive, visible focus) and appropriate ARIA attributes. This is not a one-time fix but an ongoing requirement.

Private security fixes: establish a practice for handling non-public security vulnerabilities that may never be publicly released or include legally restricted material.
