# Feature Status

**Philosophy: Default feature count is 0.** All features are disabled by default. Enable only what you need.

This document categorizes all 74 features in Argentum by their implementation status and category.

## Legend

- ✅ **Production Ready**: Fully implemented and tested
- 🚧 **TODO**: Partially implemented or needs work
- ⚠️ **Experimental**: Working but unstable API
- ❌ **Deprecated**: Planned for removal or merge

---

## Key Principle: Zero Features by Default

Argentum ships with **zero enabled features**. You explicitly enable only what you need:

```yaml
# config/default.yaml
features:
  # Start with nothing, add only what you need
  webchat:
    enabled: true
  telegram:
    enabled: true
```

**No feature is "core" or mandatory.** Even basic functionality like chat channels must be explicitly enabled.

---

## Category 1: Basic Functionality (Production Ready)

These features provide fundamental capabilities. Most users will enable at least 2-3 of these:

### Tool Calling & Automation
1. **browser-automation** ✅ - Web interaction via Playwright (custom tool calling)
2. **computer-control** ✅ - Desktop automation via vision/screen capture
3. **container-sandbox** ✅ - Docker command execution in isolated environment
4. **api-gateway** ✅ - REST API endpoint management and custom tool exposure

### Media Processing
5. **image-generation** ✅ - DALL-E/Stable Diffusion integration
6. **document-analysis** ✅ - PDF/DOCX text extraction and analysis
7. **video-processing** 🚧 - Video file analysis (TODO: complete implementation)

### Communication Channels
8. **telegram** ✅ - Telegram bot integration
9. **discord-bot** ✅ - Discord server integration
10. **webchat** ✅ - Browser-based chat interface
11. **slack-integration** ✅ - Slack workspace bot
12. **whatsapp-bridge** ✅ - WhatsApp Business API
13. **email-integration** ✅ - IMAP/SMTP email handling
14. **sms-gateway** ✅ - SMS via Twilio/Vonage
15. **mobile-push** ✅ - Push notifications to mobile apps

### Search & Web
16. **web-search** - (via browser-automation or external API)
17. **file-watcher** ✅ - Directory change monitoring

---

## Category 2: Memory & Knowledge (Production Ready)

Enable these for persistent memory and knowledge management:

18. **sqlite-memory** ✅ - Primary persistent conversation storage
19. **markdown-memory** ✅ - Human-readable notes and documents
20. **mcp-memory** ✅ - Model Context Protocol support
21. **semantic-search** ✅ - Vector similarity search
22. **knowledge-graph** ✅ - Entity relationship mapping
23. **multimodal-memory** 🚧 - Mixed media memory storage (TODO)
24. **shared-knowledge-base** 🚧 - Multi-user knowledge sharing (TODO)

---

## Category 3: Security & Governance (Production Ready)

Enable these for production deployments:

25. **content-filtering** ✅ - Input sanitization and safety
26. **rate-limiting** ✅ - Abuse prevention
27. **audit-log** ✅ - Security event tracking
28. **encrypted-secrets** ✅ - Credential storage (AES-256)
29. **allowlists** ✅ - Domain/command whitelisting
30. **role-based-access** ✅ - Permission levels
31. **tenant-isolation** 🚧 - Multi-tenant data separation (TODO: enterprise)
32. **governance** 🚧 - Policy enforcement framework (TODO)

---

## Category 4: Workflow & Automation (Production Ready)

Enable these for complex automation:

33. **mesh-workflows** ✅ - Automation engine with conditional logic
34. **cron-scheduler** ✅ - Time-based task execution
35. **webhooks** ✅ - HTTP callback handlers
36. **multi-agent-coordination** ✅ - Agent team collaboration
37. **goals** ✅ - Objective tracking system
38. **goal-decomposition** 🚧 - Automatic goal breakdown (TODO)
39. **task-checkout** ✅ - Task assignment workflow

---

## Category 5: Intelligence & Learning (TODO/Experimental)

These features provide AI-enhanced capabilities but need more work:

40. **user-modeling** 🚧 - User preference learning (TODO)
41. **skill-evolution** 🚧 - Capability improvement tracking (TODO)
42. **skills-library** ✅ - Reusable skill definitions
43. **skill-loader** ✅ - Skill loading mechanism
44. **self-improving** 🚧 - Autonomous capability enhancement (TODO)
45. **self-evolving-memory** 🚧 - Auto-organizing memory (TODO)
46. **memory-compression** 🚧 - Storage optimization (TODO)
47. **smart-recommendations** 🚧 - ML-based suggestions (TODO)

---

## Category 6: Voice & Audio (TODO)

Voice processing features need completion:

48. **voice** 🚧 - General voice processing (TODO)
49. **tts-engine** 🚧 - Text-to-speech synthesis (TODO)
50. **stt-engine** 🚧 - Speech-to-text recognition (TODO)
51. **wake-word** 🚧 - Voice activation detection (TODO)

---

## Category 7: Notifications & Digests (TODO)

These should likely be merged into a single notifications feature:

52. **weather-alerts** 🚧 - Weather notification service (TODO)
53. **news-digest** 🚧 - News aggregation (TODO)
54. **morning-briefing** 🚧 - Daily summary (TODO: merge with notifications)
55. **evening-recap** 🚧 - End-of-day summary (TODO: merge with notifications)

---

## Category 8: Developer Tools (Production Ready)

56. **auto-update** ✅ - Self-updating mechanism
57. **health-monitoring** 🚧 - System health checks (TODO)
58. **checkpoint** 🚧 - State snapshotting (TODO)
59. **trajectory-export** 🚧 - Debug export tool (TODO)
60. **budget** ✅ - Token/cost tracking
61. **sessions** ✅ - Conversation session management

---

## Category 9: Enterprise & Organization (TODO)

Enterprise features that need refinement:

62. **company-templates** 🚧 - Organization presets (TODO)
63. **org-chart** ✅ - Hierarchical structure mapping
64. **group-management** ✅ - Team organization
65. **life-domains** 🚧 - Personal life categorization (TODO)
66. **calendar-integration** ✅ - Google Calendar sync

---

## Category 10: Experimental / Unclear (TODO)

Features with unclear purpose or experimental status:

67. **live-canvas** 🚧 - Real-time collaborative whiteboard (TODO)
68. **air-gapped** 🚧 - Offline operation mode (TODO)
69. **acp-harness** 🚧 - Undefined protocol harness (TODO)
70. **auto-capture** 🚧 - Automatic data capture (TODO)
71. **consolidation** 🚧 - Data merging (TODO: vague purpose)
72. **secure-profile** 🚧 - Overlaps with encrypted-secrets (TODO)
73. **youtube-shorts** 🚧 - YouTube Shorts integration (TODO)
74. **skills-loader** ❌ - Likely duplicate of skill-loader (DEPRECATED)

---

## Feature Maturity Requirements

| Category | Test Coverage | Documentation | Stability Guarantee |
|----------|--------------|---------------|---------------------|
| Production Ready | 80%+ | Full README | SemVer respected |
| TODO | Any | Minimal | No guarantee |
| Experimental | 50%+ | Usage notes | API may change |
| Deprecated | N/A | Migration guide | Will be removed |

---

## How to Enable Features

Start with zero features, then enable what you need:

```yaml
# Minimal setup: just webchat + sqlite memory
features:
  webchat:
    enabled: true
  sqlite-memory:
    enabled: true

# Add Telegram
features:
  telegram:
    enabled: true
    # token: ${ARGENTUM_TELEGRAM_TOKEN}

# Add browser automation for web tools
features:
  browser-automation:
    enabled: true
```

---

## Migration from Old Config

If you have an existing config with features enabled, they will remain enabled. However, new installations start with zero features.

To audit your current setup:
```bash
argentum features list
```

---

Last updated: 2026-05-26
Version: v0.0.7
