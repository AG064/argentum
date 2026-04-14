# OpenClaw New Features — Integration Plan

**Branch:** `feature/openclaw-new-features`  
**Based on:** OpenClaw `2026.4.14-beta.1` analysis  
**Date:** 2026-04-14

---

## Overview

This document tracks which OpenClaw features from the `AG064/openclaw` fork are applicable to AG-Claw, with concrete integration steps and priority ordering.

---

## Priority 1: Security Fixes (Do Now)

### 1.1 Replace `marked.js` with `markdown-it`
**Why:** OpenClaw 2026.4.14 fixed a ReDoS vulnerability in `marked.js` by replacing it with `markdown-it`.  
**Action:**
```bash
# Check if AG-Claw uses marked.js
grep -r "marked" /home/agx/AGX/ag-claw --include="package.json" --include="*.ts" --include="*.js"
# Replace with markdown-it
pnpm remove marked && pnpm add markdown-it
# Update any markdown rendering code
```

### 1.2 Ollama Streaming — `stream_options.include_usage`
**Why:** Without this flag, Ollama returns bogus prompt token counts that trigger premature compaction.  
**Action:** In the Ollama transport stream handler, add:
```typescript
// In streaming completion options
stream_options: { include_usage: true }
```

---

## Priority 2: Telegram Improvements (Quick Wins)

### 2.1 Telegram Forum Topic Name Caching
**Why:** OpenClaw learns human-readable topic names from Telegram forum service messages and persists them to a session sidecar store. AG-Claw likely shows numeric topic IDs instead.  
**Code location in OpenClaw:** `src/gateway/server-methods/doctor.ts` (topic name learning); session sidecar store in `src/memory-host-sdk/host/session-files.ts`  
**Action:**
1. Intercept forum service messages (`forum_topic_created`, `forum_topic_edited`)
2. Store `{topicId: topicName}` in workspace sidecar (e.g. `.telegram-topic-names.json`)
3. On topic resolution, prefer the cached name over numeric ID

---

## Priority 3: Subagent Resilience (Medium Effort)

### 3.1 Subagent Registry Persistence
**Why:** When AG-Claw restarts, running subagents are lost. OpenClaw persists a versioned `runs.json` to `$OPENCLAW_STATE_DIR/subagents/`.  
**OpenClaw source:** `src/agents/subagent-registry.store.ts`

**AG-Claw integration:**
1. Create `src/subagent-registry/store.ts` — mirror OpenClaw's disk persistence
2. Store path: `{workspace}/.ag-claw/subagents/runs.json`
3. Version: V1 schema — `runs: Record<string, SubagentRunRecord>`
4. On startup: load registry from disk, resume any `running` subagents
5. On subagent spawn: write record immediately (not on completion)

### 3.2 Orphan Recovery
**Why:** Subagents that crash or get killed leave orphaned state. OpenClaw has `scheduleOrphanRecovery` that cleans these up.  
**Action:**
1. On registry load, identify runs stuck in `running` state
2. Schedule a recovery scan every 5 minutes
3. If a subagent session is no longer alive, mark as `failed` with `orphan: true`
4. Fire completion hooks with appropriate error

### 3.3 Steer Restart
**Why:** OpenClaw's subagent registry supports `steer restart` — restarting a subagent while preserving its conversation history and context.  
**Action:**
1. Implement `subagent restart <runId>` command
2. Persist the session transcript between restarts (already in AG-Claw's design)
3. On restart, replay recent context and resume from last checkpoint

---

## Priority 4: Memory & Dreaming (Longer Term)

### 4.1 Active Memory (Simplified)
**What:** OpenClaw's Active Memory plugin runs a small sub-agent before every main reply, automatically pulling relevant context from memory. AG-Claw doesn't have a plugin system — implement a simpler version.  
**Approach:**
1. Add a `memory/active/` directory in workspace
2. Before each agent turn, run a lightweight `memory_recall` with recent conversation summary
3. Prepend recalled memories to system prompt or inject as hidden user message
4. Make it toggleable via config

### 4.2 Dreaming (Full Implementation — Requires More Work)
**What:** Scheduled 3-phase memory consolidation (Light/Deep/REM) that synthesizes insights from sessions, logs, and memory store.  
**AG-Claw simplified approach:**

| Phase | OpenClaw | AG-Claw Simplified |
|-------|----------|-------------------|
| Light | Every 6h, 3 sources | Daily cron job, appends to `memory/daily-synthesis.md` |
| Deep | Daily 3am, 5 sources + recovery | Weekly cron job, writes `memory/deep-dreams/YYYY-MM-DD.md` |
| REM | Saturday 5am | Monthly cron job |

**Files to create:**
```
memory/
  dreams/
    light/       # light dreaming output
    deep/        # deep dreaming output
    rem/         # REM dreaming output
  daily-synthesis.md  # daily context summary
```

**Cron entry (simplified):**
```bash
# Deep dreaming — every day at 3am
0 3 * * * cd /home/agx/AGX/ag-claw && node scripts/dreaming.js --phase deep >> logs/dreaming.log 2>&1

# Light synthesis — every 4 hours
0 */4 * * * cd /home/agx/AGX/ag-claw && node scripts/dreaming.js --phase light >> logs/dreaming.log 2>&1
```

**`scripts/dreaming.js` outline:**
1. Load recent sessions from `sessions/` directory
2. Collect daily notes from `memory/` directory
3. Call LLM with phase-appropriate system prompt
4. Write output to appropriate `memory/dreams/` subdirectory
5. Log completion with token usage

---

## Priority 5: MCP Integration (Future / Conditional)

### 5.1 Plugin Tools MCP Server
**When:** Only relevant if AG-Claw gets a plugin system.

**If needed sooner:** The standalone `plugin-tools-serve.ts` pattern could expose AG-Claw's built-in tools to external Claude Code sessions without a full plugin system. E.g.:
```bash
node --import tsx src/mcp/plugin-tools-serve.ts
# Exposes: say, send_message, search_memory, etc.
```

### 5.2 Bundle MCP
**When:** Only if AG-Claw adopts plugin manifests (`.mcp.json` per plugin).  
**Files to reference:** `src/plugins/bundle-mcp.ts`, `src/plugins/bundle-manifest.ts`

---

## Implementation Order

```
1. Security: marked.js → markdown-it         (1hr, high impact)
2. Ollama: stream_options.include_usage      (15min, fixes compaction)
3. Telegram: topic name caching              (2hr, small UX win)
4. Subagent: disk persistence               (4hr, reliability)
5. Subagent: orphan recovery                (2hr, reliability)
6. Subagent: steer restart                 (3hr, UX improvement)
7. Active Memory: simplified version        (6hr, major UX improvement)
8. Dreaming: daily synthesis cron           (4hr, memory quality)
9. Dreaming: weekly deep dive               (4hr, memory quality)
10. MCP: standalone plugin-tools server     (defer, conditional)
```

---

## Files to Create/Modify

### New files
```
scripts/dreaming.js                     # dreaming runner
src/subagent-registry/store.ts         # disk persistence
src/subagent-registry/orphan-recovery.ts
memory/dreams/light/.gitkeep
memory/dreams/deep/.gitkeep
memory/dreams/rem/.gitkeep
memory/active/.gitkeep
```

### Modifications
```
package.json                            # add markdown-it, remove marked
src/agents/telegram.ts                  # add topic name caching
src/agents/ollama.ts                    # add stream_options.include_usage
src/subagent/spawn.ts                  # add registry write on spawn
src/entry.ts                            # load registry on startup
```

---

## OpenClaw Source References

| Topic | File |
|-------|------|
| Dreaming config types | `src/memory-host-sdk/dreaming.ts` |
| Dreaming test | `src/memory-host-sdk/dreaming.test.ts` |
| MCP server | `src/mcp/plugin-tools-serve.ts` |
| Bundle MCP | `src/plugins/bundle-mcp.ts` |
| Subagent store | `src/agents/subagent-registry.store.ts` |
| Subagent persistence tests | `src/agents/subagent-registry.persistence.test.ts` |
| Entry respawn | `src/entry.respawn.ts` |
| Changelog | `CHANGELOG.md` (2026.4.14 section) |
