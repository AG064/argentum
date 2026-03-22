# Tutorial 2: Memory Management

Learn how AG-Claw's multi-layered memory system works and how to configure it for your agents.

## 🧠 Memory Architecture

AG-Claw uses a **three-layer memory system**:

1. **Episodic Memory** — Stores conversation history
2. **Semantic Memory** — Stores facts and knowledge
3. **Procedural Memory** — Stores skills and procedures

## 📝 Episodic Memory

Episodic memory stores conversations and interactions.

### Configuration

```typescript
memory: {
  episodic: {
    enabled: true,
    retention: 30, // days
    compression: true,
    summarize: true, // Auto-summarize old conversations
  }
}
```

### Query Examples

```typescript
// Find recent conversations about a topic
await agent.memory.search({
  type: 'episodic',
  query: 'project planning',
  limit: 5
});

// Get conversations from specific time
await agent.memory.getByDate({
  type: 'episodic',
  start: '2026-03-01',
  end: '2026-03-15'
});
```

## 📚 Semantic Memory

Stores structured facts and knowledge.

### Adding Facts

```typescript
// Add a fact
await agent.memory.add({
  type: 'semantic',
  content: {
    fact: 'The project deadline is March 30, 2026',
    category: 'project',
    tags: ['deadline', 'project-x']
  }
});

// Add a document
await agent.memory.add({
  type: 'semantic',
  content: {
    title: 'API Documentation',
    text: 'The /api/users endpoint returns...',
    url: 'https://docs.example.com/api'
  }
});
```

### Searching Semantic Memory

```typescript
// Semantic search
const results = await agent.memory.search({
  type: 'semantic',
  query: 'deadline information',
  threshold: 0.8
});
```

## 🎯 Procedural Memory

Stores skills, procedures, and how-to guides.

### Creating a Skill

```typescript
await agent.memory.add({
  type: 'procedural',
  content: {
    name: 'Deploy Application',
    steps: [
      { action: 'build', command: 'npm run build' },
      { action: 'test', command: 'npm test' },
      { action: 'deploy', command: './scripts/deploy.sh' }
    ],
    prerequisites: ['Node.js 18+', 'Docker'],
    estimatedTime: '5 minutes'
  }
});
```

## 🔄 Memory Synchronization

Agents can share memory across sessions:

```typescript
// Enable shared memory
memory: {
  shared: {
    enabled: true,
    scope: 'team', // or 'project', 'global'
    syncInterval: 60000 // ms
  }
}
```

## 🗑️ Memory Cleanup

Automatic cleanup:

```typescript
memory: {
  cleanup: {
    enabled: true,
    schedule: '0 3 * * *', // 3 AM daily
    maxAge: 90, // days
    maxSize: '500MB'
  }
}
```

Manual cleanup:

```bash
agclaw memory cleanup --type episodic --older-than 30d
agclaw memory stats
```

## 📊 Memory Monitoring

View memory statistics:

```bash
agclaw memory stats

# Example output:
# 
# Memory Usage:
# ├─ Episodic:   127.3 MB (1,542 conversations)
# ├─ Semantic:    89.7 MB (3,891 facts)
# └─ Procedural:  23.1 MB (47 skills)
# Total:         240.1 MB
```

## 🛠️ Troubleshooting

| Issue | Solution |
|-------|----------|
| Memory growing too large | Enable compression + auto-summarize |
| Slow semantic search | Increase cache size |
| Facts not being retrieved | Lower similarity threshold |
| Memory not syncing | Check network + sync interval |

## 📖 Next Steps

- [Skill Development](../03-skill-development.md) — Create reusable skills
- [Advanced Patterns](../05-advanced-patterns.md) — Master memory patterns
