# Privacy and data handling

Argentum is local-first. It does not run a background analytics service, upload a
workspace automatically, or sync workspace contents to Argentum.

## What stays local

- Workspace files, chats, configuration, logs, local memory, and model files stay
  on the selected machine unless a feature below sends selected data elsewhere.
- Provider keys are stored in the workspace `secrets.env` file by the current
  desktop onboarding flow. Protect the local account and never commit or share it.
- Activity, audit, gateway, and local-server logs are written locally. Sensitive
  values are redacted where supported.
- System dashboard data is collected locally only while that optional tab is open.

## What can leave the machine

- A provider receives a prompt, attachments, and selected context only when the
  user starts a provider request. The provider's own privacy policy applies.
- GitHub is contacted for release checks and recovery update checks. Those checks
  send the app's HTTP request and do not send workspace contents.
- Hugging Face and other configured services are contacted only when their search,
  model download, channel, or provider features are used.
- Telegram, webchat, and other channels can send data to their configured service
  when the user enables them.

## Bug reports

Bug reports are user initiated. The GitHub report and email report actions can
include the error message, app version, platform, user agent, and reproduction
notes. They do not include workspace paths, API keys, provider tokens, chats,
attachments, or raw workspace files by default. Review the prefilled report before
sending it.

Reports can be sent through [GitHub Issues](https://github.com/AG064/argentum/issues)
or email to [report@ag064.eu](mailto:report@ag064.eu). Private security reports
should use email and must not include live credentials.

## User control

The user chooses the provider, context categories, optional system dashboard,
channels, model downloads, and whether to send a bug report. Argentum does not
claim that a provider or enabled channel will retain no data; review those
services' policies before enabling them.
