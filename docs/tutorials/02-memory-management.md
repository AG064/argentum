# Tutorial 2: Memory Management

Argentum has a sophisticated multi-layered memory system. This tutorial covers each layer in depth, shows how to work with them directly, and explains the automation that keeps memory useful over time.

**Estimated time:** 20 minutes  
**Prerequisites:** A running Argentum instance (from [Tutorial 1](./01-first-agent.md))

---

## The Four Memory Layers

Argentum organizes memory into four layers, each with a specific role:

| Layer | Technology | Purpose | Lifetime |
|---|---|---|---|
| **Session** | In-memory | Current conversation context | Until session ends |
| **Semantic** | SQLite + FTS5 | Facts, indexed by meaning | Permanent until purged |
| **Knowledge Graph** | SQLite + graph | Entity relationships | Permanent until purged |
| **Markdown** | Files on disk | Human-readable reference | Permanent, file-managed |

Messages flow through all layers during processing. Understanding each one helps you use and debug the system effectively.

---

## Layer 1: Session Memory

Session memory holds the current conversation. It's maintained in RAM during a session and persisted to `data/sessions.db` when the session ends or on checkpoint intervals.

### How It Works

When you send a message with a known `userId`, the agent retrieves that user's recent conversation history and includes it in the LLM context. This is what makes multi-turn conversations work.

```bash
# Start a conversation
curl -X POST http://localhost:18789/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "I am learning TypeScript", "userId": "alice"}'
# sessionId: sess_abc123

# Continue the conversation — session is automatically loaded
curl -X POST http://localhost:18789/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "What should I learn next?", "userId": "alice"}'
# The agent knows you are learning TypeScript
```

### Session Configuration

```json
{
  "features": {
    "sessions": {
      "enabled": true,
      "maxHistory": 50,       // Max messages per session
      "checkpointInterval": 5  // Persist every N messages
    }
  }
}
```

### Managing Sessions

```bash
# List all sessions
argentum sessions list

# View a specific session
argentum sessions view sess_abc123

# Clear a session (reset conversation)
argentum sessions clear sess_abc123

# Export session as JSON
argentum sessions export sess_abc123 > conversation.json
```

---

## Layer 2: Semantic Memory

Semantic memory stores facts and information in SQLite with full-text search. Entries are indexed so the agent can find relevant context even when search terms don't exactly match stored text.

### Storing Memories Explicitly

```bash
curl -X POST http://localhost:18789/memory/store \
  -H "Content-Type: application/json" \
  -d '{
    "content": "Alice prefers dark mode in her IDE",
    "tags": ["preference", "development"],
    "userId": "alice"
  }'
```

Response:

```json
{
  "success": true,
  "data": {
    "id": "mem_abc123",
    "createdAt": "2026-03-23T10:00:00Z"
  }
}
```

### Searching Semantic Memory

```bash
# Basic search
curl "http://localhost:18789/memory/search?q=dark%20mode"

# With limit and threshold
curl "http://localhost:18789/memory/search?q=preferences&limit=5&threshold=0.7"
```

Response:

```json
{
  "success": true,
  "data": {
    "results": [
      {
        "id": "mem_abc123",
        "content": "Alice prefers dark mode in her IDE",
        "score": 0.94,
        "createdAt": "2026-03-23T10:00:00Z",
        "tags": ["preference", "development"]
      }
    ],
    "total": 1
  }
}
```

### How the Agent Uses It

When processing a message, the agent queries semantic memory for relevant context. The `self-evolving-memory` feature also automatically captures facts from conversations. For example, if you say:

> "I just started an internship at Binarwelt"

Argentum automatically captures this as a memory entry with tags `["fact", "work", "internship"]`.

### Memory Tags

Tags help organize and filter memories. Use them when storing explicitly:

```bash
curl -X POST http://localhost:18789/memory/store \
  -H "Content-Type: application/json" \
  -d '{
    "content": "Deadline for the project is March 30th",
    "tags": ["deadline", "project"],
    "userId": "alice"
  }'
```

Query by tag:

```bash
curl "http://localhost:18789/memory/search?q=deadline&tags=project"
```

---

## Layer 3: Knowledge Graph

The knowledge graph stores entities and their relationships as a directed graph. This enables the agent to reason about connected facts.

### Entities and Relations

An entity is a node with a type and properties:

```json
{
  "id": "entity_001",
  "type": "person",
  "name": "Alice",
  "properties": {
    "location": "San Francisco, US",
    "timezone": "America/Los_Angeles"
  }
}
```

A relation is a directed edge between entities:

```json
{
  "from": "entity_001",
  "type": "works_at",
  "to": "entity_002"
}
```

### How the Graph Builds

The agent automatically extracts entities and relations from conversations. When you say:

> "I work at Acme Corp"

Argentum creates:
- Entity: `Acme Corp` (type: organization)
- Entity: `Alice` (type: person)
- Relation: Alice → `works_at` → Acme Corp

### Querying the Graph

```bash
# Get all info about a specific entity
curl "http://localhost:18789/memory/graph?entity=Alice"
```

```json
{
  "success": true,
  "data": {
    "entities": [
      {
        "id": "entity_001",
        "type": "person",
        "name": "Alice",
        "properties": {
          "location": "San Francisco, US",
          "timezone": "America/Los_Angeles"
        }
      }
    ],
    "relations": [
      { "from": "entity_001", "type": "works_at", "to": "entity_002" },
      { "from": "entity_001", "type": "speaks", "to": "entity_003" },
      { "from": "entity_001", "type": "interested_in", "to": "entity_004" }
    ]
  }
}
```

### Manual Graph Operations

Add an entity directly:

```bash
curl -X POST http://localhost:18789/memory/graph/entity \
  -H "Content-Type: application/json" \
  -d '{
    "name": "Argentum",
    "type": "project",
    "properties": {
      "version": "0.0.5",
      "language": "TypeScript"
    }
  }'
```

Add a relation:

```bash
curl -X POST http://localhost:18789/memory/graph/relation \
  -H "Content-Type: application/json" \
  -d '{
    "from": "entity_001",
    "type": "created",
    "to": "entity_005"
  }'
```

### Graph Traversal

The knowledge graph supports traversal queries. Find all people who work at the same place as Alice:

```bash
curl "http://localhost:18789/memory/graph/traverse?from=entity_001&type=works_at&depth=1"
```

---

## Layer 4: Markdown Memory

Markdown files on disk serve as a manually curated, human-readable memory layer. Argentum watches the `data/memory/` directory and automatically loads any `.md` files into context.

### Creating Memory Files

```bash
mkdir -p data/memory

cat > data/memory/personal-context.md << 'EOF'
# Personal Context

## About Alice
- Full-stack developer learning TypeScript
- Based in San Francisco, US
- Available for remote work
- timezone: America/Los_Angeles

## Current Projects
- Building an AI agent framework (Argentum)
- Job search for programming positions

## Skills
- TypeScript, JavaScript, Java, Python
- Linux, Docker, Git
- English (native), Spanish (B2)

## Goals for 2026
- Find a programming job
- Learn Rust
- Contribute to open source
EOF
```

Changes to this file are picked up immediately — no restart needed.

### How Markdown Memory Integrates

The `markdown-memory` feature parses these files and injects relevant sections into the agent's context. It's particularly useful for:
- Facts that should never be forgotten or compressed away
- Manually curated context from other systems
- Long-term goals and preferences

### Memory File Naming

Files are processed alphabetically. Use prefixes to control load order:

```
data/memory/
├── 00-identity.md      # Loaded first — core identity
├── 01-preferences.md   # User preferences
├── 02-projects.md      # Current projects
└── 03-goals.md         # Goals and aspirations
```

---

## Self-Evolving Memory

The `self-evolving-memory` feature automates memory maintenance. It runs in the background and handles three tasks:

### 1. Consolidation

When semantic memory gets large, similar entries are merged:

```
Before: 100 entries about "project meetings"
After:  3 consolidated entries about project meetings
```

This keeps context window usage manageable without losing information.

### 2. Pattern Discovery

The feature scans for recurring themes and creates summary entries:

```
Pattern detected: "weekly project sync every Monday"
Summary created: "User has recurring Monday meetings"
```

### 3. Relevance Decay

Low-relevance entries that haven't been accessed in a while gradually lose priority:

```json
{
  "feature": {
    "self-evolving-memory": {
      "enabled": true,
      "consolidationThreshold": 500,
      "decayAfterDays": 30,
      "minRelevanceScore": 0.2
    }
  }
}
```

---

## Memory Statistics and Maintenance

### View Statistics

```bash
argentum memory stats
```

Output:

```
Semantic Memory:  1247 entries
Knowledge Graph:   342 entities, 891 relations
Markdown Files:      4 files loaded
Session Memory:      8 active sessions
Compression ratio:   73%
```

### Purge Old Entries

```bash
# Remove memories older than a date
argentum memory purge --before 2026-01-01

# Purge by tag
argentum memory purge --tag temporary

# Show what would be purged without actually deleting
argentum memory purge --before 2026-01-01 --dry-run
```

### Export and Import

Export all memories for backup or migration:

```bash
argentum memory export --format json > memories-backup-$(date +%Y%m%d).json
```

Import:

```bash
argentum memory import memories-backup-20260318.json
```

---

## Debugging Memory

### Check What the Agent Retrieves

Enable debug logging to see memory queries:

```bash
AGCLAW_LOG_LEVEL=debug argentum gateway start
```

Look for log lines like:

```
[memory] Searching semantic: query="TypeScript" limit=5 threshold=0.6
[memory] Retrieved 3 results (score >= 0.6)
[memory] Graph query: entity=Alice depth=2
[memory] Graph results: 4 entities, 7 relations
```

### Verify a Memory Was Stored

```bash
curl "http://localhost:18789/memory/search?q=Alice%20TypeScript"
```

### Check Knowledge Graph Consistency

```bash
# List entities of a specific type
curl "http://localhost:18789/memory/graph?type=person"

# Find orphaned entities (entities with no relations)
curl "http://localhost:18789/memory/graph?orphans=true"
```

---

## Configuration Reference

Full memory configuration in `argentum.json`:

```json
{
  "memory": {
    "primary": "sqlite",
    "path": "./data",
    "selfEvolving": true,
    "compressionThreshold": 50000
  },
  "features": {
    "sqlite-memory": {
      "enabled": true
    },
    "semantic-search": {
      "enabled": true,
      "indexOnStartup": true
    },
    "markdown-memory": {
      "enabled": true,
      "watchPath": "./data/memory",
      "maxFileSize": 1048576
    },
    "self-evolving-memory": {
      "enabled": true,
      "consolidationIntervalHours": 6,
      "consolidationThreshold": 500,
      "decayAfterDays": 30,
      "minRelevanceScore": 0.2
    },
    "knowledge-graph": {
      "enabled": true,
      "autoExtract": true,
      "maxEntities": 10000
    },
    "sessions": {
      "enabled": true,
      "maxHistory": 50,
      "checkpointInterval": 5
    }
  }
}
```

---

## What You Learned

- The four memory layers and their roles
- How session memory enables multi-turn conversations
- How to store, search, and manage semantic memories
- How the knowledge graph builds entity relationships automatically
- How to use Markdown files as a manually curated memory layer
- How self-evolving memory automates consolidation and decay
- How to debug and maintain your memory system

---

## Next Steps

- **[Tutorial 3: Channels and Integrations](./tutorials/03-channels.md)** — Connect your agent to Discord, Slack, Email, and more
- **[Developer Guide: Creating a New Feature](../DEVELOPER_GUIDE.md#5-how-to-add-a-new-feature)** — Build custom memory processors
- **[API Reference: Memory Endpoints](../API.md#3-rest-endpoints)** — Complete endpoint documentation
