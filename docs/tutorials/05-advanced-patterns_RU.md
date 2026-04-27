# Урок 5: Продвинутые паттерны

*Примерное время: 35 минут*

Этот урок охватывает продвинутые паттерны Argentum: мультиагентную координацию, mesh-workflows, оркестрацию задач и масштабирование для high-volume развёртываний.

---

## Мультиагентная координация

Argentum поддерживает запуск нескольких специализированных агентов, которые работают вместе. Это полезно для сложных задач, которые выигрывают от узкой экспертизы.

### Архитектура

```
┌─────────────────────────────────────────────────────────────┐
│                    Coordinator Agent                         │
│  (понимает намерение, делегирует, синтезирует результаты)    │
└──────────────────┬──────────────────────────────────────────┘
                   │
       ┌───────────┼───────────┬──────────────┐
       ▼           ▼           ▼              ▼
┌────────────┐ ┌──────────┐ ┌─────────┐ ┌──────────┐
│  Coding   │ │ Research │ │ Review  │ │   Ops    │
│  Agent    │ │  Agent   │ │ Agent   │ │  Agent   │
│           │ │          │ │         │ │          │
│ Пишет     │ │ Ищет     │ │ Проверяет│ │ Мониторит│
│ код       │ │ документы│ │ качество│ │ системы  │
└────────────┘ └──────────┘ └─────────┘ └──────────┘
```

### Настройка нескольких агентов

В `argentum.json`:

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

### Как работает координация

Когда пользователь отправляет сообщение координатору:

1. **Анализ намерения**: Координатор анализирует запрос
2. **Декомпозиция задачи**: Координатор разбивает на подзадачи
3. **Делегирование**: Координатор назначает подзадачи специализированным агентам
4. **Выполнение**: Специалисты работают параллельно
5. **Синтез**: Координатор собирает результаты и формирует единый ответ

```
Пользователь: Построй REST API для todo-приложения с аутентификацией

Координатор:
  1. Планирование: Нужен backend-агент (дизайн API, аутентификация) и агент ревью
  2. Делегирует агенту "coding"
  3. Агент coding пишет: routes, middleware, auth, схему базы данных
  4. Координатор синтезирует финальный ответ
```

### Коммуникация агентов

Агенты общаются через общую память и систему передачи сообщений:

```bash
# От агента coding к координатору
await context.emit('agent:message', {
  to: 'coordinator',
  from: 'coding',
  type: 'task_complete',
  payload: { taskId: 'build-api', result: { files: [...], summary: '...' } }
});
```

---

## Mesh Workflows

Mesh workflows позволяют определять многошаговые автоматизационные pipelines с использованием JSON-based языка выражений (jsep).

### Что такое Mesh Workflows

Mesh workflow — это направленный граф, где узлы это задачи, а рёбра определяют зависимости. Планировщик выполняет задачи в топологическом порядке, с параллельным выполнением где возможно.

### Определение Workflow

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

### Более сложный Workflow с условиями

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

### Запуск Workflow

```bash
# Через CLI
argentum workflow run deploy-service

# Через API
curl -X POST http://localhost:3000/workflows/run \
  -H "Content-Type: application/json" \
  -d '{"workflow": "deploy-service", "params": {}}'

# Через инструмент агента
Агент: run_workflow(name="deploy-service")
```

---

## Cron-планирование

Планируйте задачи для автоматического выполнения в определённое время.

### Настройка Cron-заданий

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
      "prompt": "Сгенерируй утренний брифинг на сегодня",
      "enabled": true
    },
    {
      "id": "database-backup",
      "cron": "0 2 * * *",
      "action": "run_command",
      "command": "pg_dump -U argentum argentum_db > /backups/argentum-$(date +%Y%m%d).sql",
      "enabled": true
    },
    {
      "id": "weekly-report",
      "cron": "0 9 * * 1",
      "action": "agent:prompt",
      "agent": "coordinator",
      "prompt": "Сгенерируй еженедельное резюме всей активности и отправь на email"
    }
  ]
}
```

### Формат Cron-выражения

```
┌───────────── минута (0-59)
│ ┌───────────── час (0-23)
│ │ ┌───────────── день месяца (1-31)
│ │ │ ┌───────────── месяц (1-12)
│ │ │ │ ┌───────────── день недели (0-6, воскресенье=0)
│ │ │ │ │
* * * * *
```

Частые паттерны:
- `0 8 * * *` — каждый день в 8:00 утра
- `0 */2 * * *` — каждые 2 часа
- `0 9 * * 1` — каждый понедельник в 9:00
- `*/15 * * * *` — каждые 15 минут

### Atomic Task Checkout

Для критических запланированных задач cron-планировщик использует atomic checkout для предотвращения дублирующих выполнений в кластерных окружениях:

```typescript
// Только один экземпляр выполняет эту задачу, даже с несколькими репликами
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

Получайте входящие события от внешних систем.

### Настройка Webhooks

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

### Отправка исходящих Webhooks

```bash
curl -X POST http://localhost:3000/webhooks/outgoing \
  -H "Content-Type: application/json" \
  -d '{"event": "deploy-complete", "data": {"service": "api", "version": "v1.2.3"}}'
```

---

## Масштабирование для высокой нагрузки

### Stateless Gateway

Gateway Argentum спроектирован быть преимущественно stateless. Для горизонтального масштабирования:

1. **Stateless компоненты**: Gateway сам по себе не хранит состояние
2. **Общее состояние**: Память (SQLite), сессии и конфиги используют общее хранилище
3. **Stateless функции**: `semantic-search`, `knowledge-graph` могут использовать внешние бэкенды (Supabase)

### Внешний Memory Backend (Supabase)

Для мультиинстансных развёртываний используйте Supabase как общий memory backend:

```json
{
  "memory": {
    "primary": "supabase",
    "supabaseUrl": "https://xxx.supabase.co",
    "supabaseKey": "${SUPABASE_KEY}"
  }
}
```

Это позволяет нескольким инстансам gateway использовать одну и ту же память.

### Load Balancing

```yaml
# docker-compose.yml для масштабирования
services:
  argentum-1:
    image: argentum:latest
    deploy:
      replicas: 3
    # Все инстансы используют один и тот же memory backend
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

Для распределённого выполнения задач между несколькими инстансами агента.

### Как это работает

```typescript
const checkout = await taskCheckout({
  taskName: 'data-sync',
  ttlMs: 300_000,  // 5 минут
  heartbeatMs: 30_000,
});

// Если мы получили блокировку
if (checkout.acquired) {
  try {
    await performDataSync();
    await checkout.complete();
  } catch (err) {
    await checkout.fail(err.message);
  }
}
```

### Особенности

- **Atomic lock acquisition**: Только один инстанс получает задачу
- **Heartbeat**: Если воркер умирает, блокировка истекает после `ttlMs`
- **Retry**: Другие инстансы могут повторить после истечения блокировки
- **Idempotency**: Задачи спроектированы быть безопасными для повтора

---

## Мониторинг продвинутых Workflows

### Workflow Metrics

Когда mesh-workflows активны, доступны дополнительные метрики:

```
# HELP argentum_workflow_steps_total Workflow step executions
# TYPE argentum_workflow_steps_total counter
argentum_workflow_steps_total{workflow="deploy-service", step="build", result="success"} 45
argentum_workflow_steps_total{workflow="deploy-service", step="build", result="failure"} 2

# HELP argentum_workflow_duration_seconds Workflow execution time
# TYPE argentum_workflow_duration_seconds histogram
argentum_workflow_duration_seconds{workflow="deploy-service"} 234.5
```

### Трассировка взаимодействий агентов

Включите трассировку для отладки мультиагентных взаимодействий:

```bash
AGCLAW_TRACE=agent,workflow npm start
```

Это выводит детальные трассы передачи сообщений агентов и выполнения workflow.

---

## Лучшие практики

### 1. Делайте агентов сфокусированными

Не пытайтесь сделать одного агента для всего. Специализированные агенты с чёткими обязанностями превосходят универсальных агентов.

### 2. Используйте идемпотентные операции

Workflows и запланированные задачи должны быть безопасны для многократного запуска:

```typescript
// Bad: создаёт дублирующие записи
await db.insert('logs', { message: 'started' });

// Good: проверяет перед вставкой
const exists = await db.query('SELECT 1 FROM logs WHERE job_id = ?', [jobId]);
if (!exists) {
  await db.insert('logs', { job_id: jobId, message: 'started' });
}
```

### 3. Устанавливайте правильные таймауты

Долгие задачи должны иметь явные таймауты:

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

### 4. Следите за ростом памяти

В долгих развёртываниях мониторьте размер семантической памяти:

```bash
argentum memory stats
# Если entries > compressionThreshold, должна запуститься консолидация
```

### 5. Используйте правильный инструмент

| Тип задачи | Лучший подход |
|---|---|
| Разовая сложная задача | Агент с инструментами |
| Повторяющаяся задача | Cron schedule |
| Событийно-управляемая | Webhook |
| Multi-step pipeline | Mesh workflow |
| Распределённая координация | Task checkout |

---

## Следующие шаги

- **[Справочник API](../API.md)** — Полная документация REST и WebSocket API
- **[Руководство разработчика](../DEVELOPER_GUIDE.md)** — Вклад в проект, тестирование, процесс релизов

---

*Вопросы? Создайте issue на [GitHub](https://github.com/AG064/argentum/issues).*
