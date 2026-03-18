# Skills Audit — AG-Claw (2026-03-18)

## Installed Skills (16)

| # | Skill | Description | Status |
|---|-------|-------------|--------|
| 1 | actual-self-improvement | Capture durable lessons from debugging, corrections | ✅ Keep (chosen over self-improving) |
| 2 | agent-task-tracker | Proactive task state management | ✅ Keep |
| 3 | automation-workflows | Design automation workflows (Zapier, Make, n8n) | ✅ Keep |
| 4 | auto-updater | Auto-update OpenClaw and skills daily | ✅ Keep |
| 5 | calendar | Calendar management and scheduling | ✅ Restored (was deleted by mistake) |
| 6 | coding | Coding assistance | ✅ Keep |
| 7 | deep-research-pro | Multi-source deep research agent | ✅ Keep |
| 8 | deep-scraper | Web scraping with Docker/Crawlee | ✅ Keep |
| 9 | elevenlabs-api | ElevenLabs TTS integration | ✅ Keep |
| 10 | free-ride | Manages free AI models from OpenRouter | ✅ Keep |
| 11 | humanizer | Remove AI writing patterns (v2.1.1) | ✅ Keep (chosen over ai-humanizer) |
| 12 | summarize | Summarize URLs/files (web, PDFs, audio, YouTube) | ✅ Keep |
| 13 | telegram | Telegram Bot API workflows | ✅ Keep |
| 14 | translate | Accurate text translation | ✅ Keep |
| 15 | writing-assistant | Writing team lead with specialized writers | ✅ Keep |
| 16 | x-twitter | Twitter/X interaction (read, post, search) | ✅ Keep |

## Removed Duplicates

| Removed | Kept | Reason |
|---------|------|--------|
| self-improving (v1.2.16) | actual-self-improvement (v1.0.0) | More structured logging, integrates with workspace .learnings/ |
| ai-humanizer (v2.1.0) | humanizer (v2.1.1) | Newer version, more detailed patterns |

## Skill vs Feature Mapping

| Skill | AG-Claw Feature | Overlap |
|-------|-----------------|---------|
| elevenlabs-api | voice | Partial — skill is CLI wrapper, feature is API |
| telegram | (channel) | Skill is workflow design, not channel integration |
| deep-scraper | browser-automation | Complementary — scraper uses browser automation |
| summarize | (no direct feature) | Unique capability |
| calendar | (no direct feature) | Unique capability |
| translate | (no direct feature) | Unique capability |

## AG-Claw Features Without Skills

| Feature | Status | Notes |
|---------|--------|-------|
| life-domains | Placeholder | Needs implementation |
| skills-library | Placeholder | Needs implementation |
| goal-decomposition | Placeholder | Needs implementation |
| mesh-workflows | Implemented | jsep parser added |
| container-sandbox | Implemented | whitelist added |
| webchat | Implemented | token auth added |
| webhooks | Implemented | SSRF protection added |
