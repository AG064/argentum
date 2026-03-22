# Tutorial 2: Memory Management

*Estimated time: 20 minutes*

AG-Claw has one of the most sophisticated memory systems of any agent framework. This tutorial explains exactly how it works and how to use it to build agents that remember, learn, and improve over time.

---

## The Four Memory Layers

AG-Claw uses four distinct memory systems, each optimized for a different purpose:

```
┌─────────────────────────────────────────────────────────────────┐
│  Layer 1: Semantic Memory         (fast, searchable, abundant)  │
│  Stores: conversations, decisions, lessons, errors, preferences│
│  Backend: SQLite + embeddings                                    │
├─────────────────────────────────────────────────────────────────┤
│  Layer 2: Knowledge Graph          (structured, relational)       │
│  Stores: entities, relationships, facts                           │
│  Backend: SQLite with graph traversal                            │
├─────────────────────────────────────────────────────────────────┤
│  Layer 3: Markdown Memory          (human-readable, persistent)  │
│  Stores: notes, documentation, long-term facts                   │
│  Backend: Files on disk, watched for changes                     │
├─────────────────────────────────────────────────────────────────┤
│  Layer 4: Self-Evolving Memory     (compressed, abstracted)      │
│  Stores: consolidated summaries of old memories                   │
│  Backend: Automatic consolidation process                          │
└─────────────────────────────────────────────────────────────────┘
```

---

## Layer 1: Semantic Memory

The primary working memory. Every interaction, decision, lesson, and error gets stored here.

### Memory Types

| Type | When it's used | Example |
|---|---|---|
| `decision` | You make a choice about something | "We agreed to use PostgreSQL" |
| `lesson` | You learned something valuable | "Never run `rm -rf /` on a production server" |
| `error` | Something went wrong and you fixed it | "The API was returning 500 because of a missing index" |
| `preference` | User's personal preference | "Aleksey prefers Rust over Go" |
| `general` | Anything else worth remembering | Meeting notes, facts, context |

### Auto-Capture

AG-Claw automatically detects and stores important information from your messages. When you say something like:

- "Remember that..." → stored as `general`
- "We decided to..." → stored as `decision`
- "I learned that..." → stored as `lesson`
- "The bug was caused by..." → stored as `error`
- "I prefer..." → stored as `preference`

The auto-capture feature parses your messages and extracts these patterns automatically.

### Manual Memory Operations

```bash
# Store something manually
agclaw memory store "decision" "Use feature flags instead of branches for gradual rollouts"

# Search memory
agclaw memory search "feature flags"

# View recent memories
agclaw memory recent --limit 10

# View memory statistics
agclaw memory stats
# Output:
# Total entries: 1543
# By type:
#   decision: 234
#   lesson: 567
#   error: 89
#   preference: 123
#   general: 530
```

### Using Memory as an Agent Tool

The agent can use memory tools directly during conversation:

```
User: What errors have we hit in the API?
Agent: [calls memory_search with query="API errors"]
     Found 2 relevant entries:
     [error] API returned 500 due to missing index on user_id column (2026-03-20)
     [error] Rate limiter was blocking legitimate requests after the last deploy (2026-03-18)

User: Good. And what decisions did we make about the database?
Agent: [calls memory_search with type="decision", query="database"]
     Found 3 decisions:
     [decision] Use PostgreSQL as primary database (2026-03-15)
     [decision] Add Redis for caching session data (2026-03-16)
     [decision] Run database migrations at startup, not deployment (2026-03-18)
```

---

## Layer 2: Knowledge Graph

The knowledge graph stores entities and their relationships as a graph structure. This enables reasoning about connections between facts.

### What Gets Stored

- **Nodes**: People, projects, companies, technologies, concepts
- **Edges**: Relationships between nodes (`works_at`, `depends_on`, `uses`, `created_by`)

### Example Graph

```
(User) ──(created)──► (Project: AG-Claw)
   │                      │
   │                      ├──(uses)──► (Technology: TypeScript)
   │                      │
   │                      ├──(uses)──► (Technology: SQLite)
   │                      │
   │                      └──(developed_by)──► (Person: AG064)
```

### Querying the Graph

```bash
# View graph statistics
agclaw memory graph stats
# Output:
# Nodes: 342
# Edges: 891
# Most connected: AG-Claw (12 connections)

# Export the graph
agclaw memory graph export > graph.json
```

### How the Graph Updates

When semantic memory stores a new entry, the knowledge graph feature analyzes it:

1. Extracts named entities (people, places, technologies)
2. Identifies relationships between entities
3. Creates or updates graph nodes and edges
4. Links new information to existing knowledge

This means the graph grows smarter over time as the agent processes more information.

---

## Layer 3: Markdown Memory

Markdown memory stores human-readable notes as `.md` files on disk. This layer is for information you want to:

- Keep permanently
- Edit manually
- Have version-controlled in git
- Share with other tools

### How It Works

Create or edit files in the `memory/` directory:

```bash
# The memory directory
ls memory/

# memory/projects.md
# memory/people.md
# memory/notes.md
```

Example `memory/projects.md`:

```markdown
# Projects

## AG-Claw
- Type: AI Agent Framework
- Status: Active development
- Lead: AG064
- Tech: TypeScript, SQLite, Claude
- URL: https://github.com/AG064/ag-claw

## Personal Website
- Type: Static site (Astro)
- Status: Completed
- URL: https://example.com
```

The markdown-memory feature watches these files for changes and automatically updates the agent's context when relevant files are modified.

### Integrating with Agent

The agent can read markdown memory files when relevant to the conversation:

```
User: What projects are we working on?
Agent: [calls read_file on memory/projects.md]
     Based on your project notes, you're working on:
     1. AG-Claw — an AI agent framework (active)
     2. Personal Website — static site (completed)
```

---

## Layer 4: Self-Evolving Memory

Over time, semantic memory accumulates thousands of entries. The self-evolving memory feature periodically consolidates old entries to keep the system efficient.

### How Consolidation Works

When the memory exceeds `compressionThreshold` (default: 10,000 entries):

1. **Analysis**: Identifies clusters of related entries
2. **Abstraction**: Replaces multiple specific entries with generalized summaries
3. **Pruning**: Removes low-value or redundant entries
4. **Preservation**: Keeps the essential information in condensed form

### Example

Before consolidation (12 separate entries):
- "Meeting with Bob about API design — use REST" (Mar 1)
- "Bob confirmed REST is the right choice" (Mar 2)
- "Discussed REST with Bob, agreed on /api/v1 prefix" (Mar 3)
- ... (9 more similar entries)

After consolidation (1 abstracted entry):
- "Decided on REST API with /api/v1 prefix. Bob was involved in the decision." (Mar 1-15)

### Configuring Consolidation

```json
{
  "features": {
    "self-evolving-memory": {
      "enabled": true,
      "compressionThreshold": 10000,
      "consolidationIntervalHours": 24,
      "minEntriesBeforeConsolidation": 5000
    }
  }
}
```

### Checkpoint System

For long-running tasks, use checkpoints to save progress:

```bash
# Agent calls memory_checkpoint during a complex task
Agent: memory_checkpoint(taskId="refactor-auth-2026", state={
  "step": 3,
  "completed": ["login-form", "session-store"],
  "current": "token-validation",
  "remaining": ["refresh-token", "logout"]
})

# Later, resume from the checkpoint
Agent: memory_resume(taskId="refactor-auth-2026")
# Returns: { step: 3, completed: [...], current: "token-validation", ... }
```

---

## Using Memory Effectively

### Best Practices

1. **Be explicit when storing important information**
   Instead of hoping auto-capture catches something:
   ```
   User: Remember to always run tests before pushing.
   Agent: [calls memory_store with type="lesson", content="Always run tests before pushing"]
   ```

2. **Use the right memory type**
   Don't store everything as `general`. Use `decision`, `lesson`, `error`, and `preference` types — they make searching more precise.

3. **Query with natural language**
   The semantic search understands context:
   ```
   agclaw memory search "things that went wrong with the database migration"
   ```

4. **Let the graph build naturally**
   You don't need to manually populate the knowledge graph. It grows as you use memory — just store semantic entries and the graph feature will extract entities and relationships.

5. **Use markdown for permanent knowledge**
   Facts that shouldn't change (like project details, team structure) belong in markdown memory, not semantic memory.

### Memory in Multi-Agent Setups

When running multiple agents, memory can be shared or isolated:

```json
{
  "agents": [
    {
      "id": "coding-assistant",
      "memory": { "shared": false, "namespace": "coding" }
    },
    {
      "id": "research-assistant",
      "memory": { "shared": false, "namespace": "research" }
    },
    {
      "id": "coordinator",
      "memory": { "shared": true }
    }
  ]
}
```

The coordinator agent can see all memories, while specialized agents only see their own namespace.

---

## Troubleshooting Memory Issues

### Memory search returns no results

1. Check that `semantic-search` is enabled
2. Verify entries exist: `agclaw memory stats`
3. Try a broader query: `agclaw memory search "the"`
4. Store a test entry: `agclaw memory store "test" "testing memory"`

### Too many duplicate entries

The consolidation feature should handle this. If it's not running:
1. Check that `self-evolving-memory` is enabled
2. Verify `compressionThreshold` is set appropriately
3. Manually trigger consolidation: check feature health

### Memory slowing down the agent

1. Reduce `messageHistory` in webchat config (fewer conversation messages in memory)
2. Increase `compressionThreshold` to trigger consolidation sooner
3. Disable features you don't use (they may be storing unnecessary data)

---

## Next Steps

- **[Tutorial 3: Skill Development](./03-skill-development.md)** — Create custom tools that interact with memory or add new capabilities
- **[Tutorial 4: Deployment](./04-deployment.md)** — Deploy with Docker and configure for production
- **[Tutorial 5: Advanced Patterns](./05-advanced-patterns.md)** — Multi-agent coordination, mesh workflows

---

*Questions? Open an issue on [GitHub](https://github.com/AG064/ag-claw/issues).*
