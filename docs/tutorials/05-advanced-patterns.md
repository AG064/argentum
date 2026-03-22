# Tutorial 5: Advanced Patterns

*Estimated time: 35 minutes*

This tutorial covers advanced AG-Claw patterns: multi-agent coordination, mesh workflows, task orchestration, and scaling for high-volume deployments.

---

## Multi-Agent Coordination

AG-Claw supports running multiple specialized agents that work together. This is useful for complex tasks that benefit from focused expertise.

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Coordinator Agent                         │
│  (understands intent, delegates, synthesizes results)         │
└──────────────────┬──────────────────────────────────────────┘
                   │
       ┌───────────┼───────────┬──────────────┐
       ▼           ▼           ▼              ▼
┌────────────┐ ┌──────────┐ ┌─────────┐ ┌──────────┐
│  Coding    │ │Research  │ │ Review  │ │  Ops     │
│  Agent     │ │ Agent    │ │ Agent   │ │  Agent   │
│            │ │          │ │         │ │          │
│ Writes     │ │ Searches │ │ Checks  │ │ Monitors │
│ code       │ │ docs     │ │ quality │ │ systems  │
└────────────┘ └──────────┘ └─────────┘ └──────────┘
```

### Setting Up Multiple Agents

In `agclaw.json`:

```json
{
  "agents": [
    {
      "id": "coordinator",
      "name": "Coordinator",
      "model": "anthropic/claude-sonnet-4-20250514",
      "role": "coordinator",
      "memory": { "shared": true }
    },
    {
      "id": "coding",
      "name": "Coding Assistant",
      "model": "openai/gpt-4o",
      "role": "specialist",
      "systemPrompt": "You are an expert programmer. You write clean, efficient code...",
      "memory": { "shared": false, "namespace": "coding" },
      "tools": ["read_file", "write_file", "run_command", "git_status"]
    },
    {
      "id": "research",
      "name": "Research Assistant",
      "model": "anthropic/claude-sonnet-4-20250514",
      "role": "specialist",
      "systemPrompt": "You are a research specialist. You search for information, analyze papers...",
      "memory": { "shared": false, "namespace": "research" },
      "tools": ["web_search", "memory_search"]
    }
  ],
  "features": {
    "multi-agent-coordination": {
      "enabled": true,
      "coordinationModel": "coordinator",
      "maxConcurrentAgents": 3,
      "timeoutMs": 30000
    }
  }
}
```

### How Coordination Works

When a user sends a message to the coordinator:

1. **Intent Analysis**: Coordinator analyzes the request
2. **Task Decomposition**: Coordinator breaks it into subtasks
3. **Delegation**: Coordinator assigns subtasks to specialist agents
4. **Execution**: Specialists work in parallel
5. **Synthesis**: Coordinator collects results and produces a unified response

```
User: Build a REST API for a todo app with authentication

Coordinator:
  1. Planning: This needs a backend agent (API design, auth) and a review agent
  2. Delegates to "coding" agent
  3. coding agent writes: routes, middleware, auth, database schema
  4. Coordinator synthesizes the final response
```

### Agent Communication

Agents communicate through shared memory and a message-passing system:

```bash
# From coding agent to coordinator
await context.emit('agent:message', {
  to: 'coordinator',
  from: 'coding',
  type: 'task_complete',
  payload: { taskId: 'build-api', result: { files: [...], summary: '...' } }
});
```

---

## Mesh Workflows

Mesh workflows let you define multi-step automation pipelines using a JSON-based expression language (jsep).

### What Are Mesh Workflows?

A mesh workflow is a directed graph where nodes are tasks and edges define dependencies. The scheduler executes tasks in topological order, with parallel execution where possible.

### Workflow Definition

```json
{
  "workflows": {
    "deploy-service": {
      "name": "Deploy Service",
      "steps": [
        {
          "id": "build",
          "type": "task",
          "action": "run_command",
          "params": { "command": "npm run build" },
          "next": ["test"]
        },
        {
          "id": "test",
          "type": "task",
          "action": "run_command",
          "params": { "command": "npm test" },
          "next": ["deploy"]
        },
        {
          "id": "deploy",
          "type": "task",
          "action": "run_command",
          "params": { "command": "kubectl apply -f k8s/" },
          "next": []
        }
      ]
    }
  }
}
```

### More Complex Workflow with Conditions

```json
{
  "workflows": {
    "process-pr": {
      "name": "Process Pull Request",
      "steps": [
        {
          "id": "check-ci",
          "type": "task",
          "action": "run_command",
          "params": { "command": "gh run list --workflow=ci.yml --head=$PR_BRANCH" }
        },
        {
          "id": "ci-passed?",
          "type": "condition",
          "expression": "check-ci.status == 'success'",
          "then": ["review"],
          "else": ["notify-failure"]
        },
        {
          "id": "review",
          "type": "agent",
          "agent": "review",
          "prompt": "Review this PR for code quality: {{pr.diff}}"
        },
        {
          "id": "notify-failure",
          "type": "task",
          "action": "send_notification",
          "params": { "message": "CI failed for PR #{{pr.number}}" }
        }
      ]
    }
  }
}
```

### Running a Workflow

```bash
# Via CLI
agclaw workflow run deploy-service

# Via API
curl -X POST http://localhost:3000/workflows/run \
  -H "Content-Type: application/json" \
  -d '{"workflow": "deploy-service", "params": {}}'

# Via agent tool
Agent: run_workflow(name="deploy-service")
```

---

## Cron Scheduling

Schedule tasks to run automatically at specific times.

### Setting Up Cron Jobs

```json
{
  "features": {
    "cron-scheduler": {
      "enabled": true
    }
  },
  "schedules": [
    {
      "id": "morning-briefing",
      "cron": "0 8 * * *",
      "action": "agent:prompt",
      "agent": "coordinator",
      "prompt": "Generate a morning briefing for today",
      "enabled": true
    },
    {
      "id": "database-backup",
      "cron": "0 2 * * *",
      "action": "run_command",
      "command": "pg_dump -U agclaw agclaw_db > /backups/agclaw-$(date +%Y%m%d).sql",
      "enabled": true
    },
    {
      "id": "weekly-report",
      "cron": "0 9 * * 1",
      "action": "agent:prompt",
      "agent": "coordinator",
      "prompt": "Generate a weekly summary of all activity and send it to email"
    }
  ]
}
```

### Cron Expression Format

```
┌───────────── minute (0-59)
│ ┌───────────── hour (0-23)
│ │ ┌───────────── day of month (1-31)
│ │ │ ┌───────────── month (1-12)
│ │ │ │ ┌───────────── day of week (0-6, Sunday=0)
│ │ │ │ │
* * * * *
```

Common patterns:
- `0 8 * * *` — every day at 8:00 AM
- `0 */2 * * *` — every 2 hours
- `0 9 * * 1` — every Monday at 9:00 AM
- `*/15 * * * *` — every 15 minutes

### Atomic Task Checkout

For critical scheduled tasks, the cron scheduler uses atomic checkout to prevent duplicate executions in clustered environments:

```typescript
// Only one instance runs this task, even with multiple replicas
const task = await scheduler.checkout('database-backup');
if (task.acquired) {
  await runBackup();
  await task.complete();
} else {
  console.log('Task already running on another instance');
}
```

---

## Webhooks

Receive incoming events from external systems.

### Configuring Webhooks

```json
{
  "features": {
    "webhooks": {
      "enabled": true
    }
  },
  "webhooks": {
    "incoming": {
      "/webhook/github": {
        "verify": "github",
        "secret": "${GITHUB_WEBHOOK_SECRET}",
        "handler": "agent:prompt",
        "agent": "coordinator",
        "prompt": "Process this GitHub webhook: {{event}}"
      },
      "/webhook/stripe": {
        "verify": "stripe",
        "secret": "${STRIPE_WEBHOOK_SECRET}",
        "handler": "run_command",
        "command": "process-payment.sh"
      }
    },
    "outgoing": {
      "deploy-complete": {
        "url": "https://ci.example.com/webhook/deploy",
        "secret": "${CI_WEBHOOK_SECRET}",
        "events": ["workflow.complete"]
      }
    }
  }
}
```

### Sending Outgoing Webhooks

```bash
curl -X POST http://localhost:3000/webhooks/outgoing \
  -H "Content-Type: application/json" \
  -d '{"event": "deploy-complete", "data": {"service": "api", "version": "v1.2.3"}}'
```

---

## Scaling for High Volume

### Stateless Gateway

AG-Claw's gateway is designed to be mostly stateless. For horizontal scaling:

1. **Stateless components**: The gateway itself stores no state
2. **Shared state**: Memory (SQLite), sessions, and configs use shared storage
3. **Stateless features**: `semantic-search`, `knowledge-graph` can use external backends (Supabase)

### External Memory Backend (Supabase)

For multi-instance deployments, use Supabase as a shared memory backend:

```json
{
  "memory": {
    "primary": "supabase",
    "supabaseUrl": "https://xxx.supabase.co",
    "supabaseKey": "${SUPABASE_KEY}"
  }
}
```

This allows multiple gateway instances to share the same memory.

### Load Balancing

```yaml
# docker-compose.yml for scaling
services:
  ag-claw-1:
    image: ag-claw:latest
    deploy:
      replicas: 3
    # All instances share the same memory backend
    environment:
      - SUPABASE_URL=${SUPABASE_URL}
      - SUPABASE_KEY=${SUPABASE_KEY}
```

### Rate Limiting Per User

```json
{
  "security": {
    "rateLimit": {
      "enabled": true,
      "windowMs": 60000,
      "maxRequests": 30,
      "perUser": true
    }
  }
}
```

---

## Task Checkout System

For distributed task execution across multiple agent instances.

### How It Works

```typescript
const checkout = await taskCheckout({
  taskName: 'data-sync',
  ttlMs: 300_000,  // 5 minutes
  heartbeatMs: 30_000,
});

// If we got the lock
if (checkout.acquired) {
  try {
    await performDataSync();
    await checkout.complete();
  } catch (err) {
    await checkout.fail(err.message);
  }
}
```

### Features

- **Atomic lock acquisition**: Only one instance gets the task
- **Heartbeat**: If a worker dies, the lock expires after `ttlMs`
- **Retry**: Other instances can retry after the lock expires
- **Idempotency**: Tasks are designed to be safe to retry

---

## Monitoring Advanced Workflows

### Workflow Metrics

When mesh-workflows are active, additional metrics are available:

```
# HELP agclaw_workflow_steps_total Workflow step executions
# TYPE agclaw_workflow_steps_total counter
agclaw_workflow_steps_total{workflow="deploy-service", step="build", result="success"} 45
agclaw_workflow_steps_total{workflow="deploy-service", step="build", result="failure"} 2

# HELP agclaw_workflow_duration_seconds Workflow execution time
# TYPE agclaw_workflow_duration_seconds histogram
agclaw_workflow_duration_seconds{workflow="deploy-service"} 234.5
```

### Tracing Agent Interactions

Enable tracing for multi-agent debugging:

```bash
AGCLAW_TRACE=agent,workflow npm start
```

This outputs detailed traces of agent message passing and workflow execution.

---

## Best Practices

### 1. Keep Agents Focused

Don't try to make one agent do everything. Specialized agents with clear responsibilities outperform general-purpose agents.

### 2. Use Idempotent Operations

Workflows and scheduled tasks should be safe to run multiple times:

```typescript
// Bad: Creates duplicate records
await db.insert('logs', { message: 'started' });

// Good: Checks before inserting
const exists = await db.query('SELECT 1 FROM logs WHERE job_id = ?', [jobId]);
if (!exists) {
  await db.insert('logs', { job_id: jobId, message: 'started' });
}
```

### 3. Set Appropriate Timeouts

Long-running tasks should have explicit timeouts:

```json
{
  "workflows": {
    "long-task": {
      "timeoutMs": 300000,
      "retry": { "maxAttempts": 3, "backoffMs": 5000 }
    }
  }
}
```

### 4. Monitor Memory Growth

In long-running deployments, monitor semantic memory size:

```bash
agclaw memory stats
# If entries > compressionThreshold, consolidation should trigger
```

### 5. Use the Right Tool

| Task Type | Best Approach |
|---|---|
| One-time complex task | Agent with tools |
| Recurring task | Cron schedule |
| Event-driven | Webhook |
| Multi-step pipeline | Mesh workflow |
| Distributed coordination | Task checkout |

---

## Next Steps

- **[API Reference](../API.md)** — Full REST and WebSocket API documentation
- **[Developer Guide](../DEVELOPER_GUIDE.md)** — Contributing, testing, release process

---

*Questions? Open an issue on [GitHub](https://github.com/AG064/ag-claw/issues).*
