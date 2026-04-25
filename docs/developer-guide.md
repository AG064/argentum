# AG-Claw Developer Guide

Добро пожаловать в AG-Claw — модульный AI Agent Framework на базе OpenClaw. Это руководство для разработчиков, которые хотят понимать, расширять и вносить вклад в проект.

---

## 1. Начало работы

### 1.1 Запуск проекта

#### Быстрый старт

```bash
# 1. Клонируйте репозиторий
git clone https://github.com/AG064/ag-claw.git
cd ag-claw

# 2. Установите зависимости
npm install

# 3. Настройте конфигурацию
cp .env.example .env
nano .env  # Добавьте ваши API ключи

# 4. Запустите в режиме разработки
npm run dev

# Или соберите и запустите в продакшене
npm run build
npm start
```

По умолчанию AG-Claw будет доступен на `http://localhost:18789`.

#### Зависимости

| Компонент | Минимальная версия | Проверка |
|---|---|---|
| Node.js | >= 20.0 | `node -v` |
| npm | >= 9.0 | `npm -v` |
| Docker (опционально) | >= 24.0 | `docker --version` |

### 1.2 Конфигурация

#### YAML конфигурация (`config/default.yaml`)

Основной файл конфигурации управляет сервером, моделями, фичами, памятью и безопасностью:

```yaml
# Сервер Gateway
server:
  port: 18789
  host: "0.0.0.0"
  cors:
    enabled: true
    origins: ["*"]
  rateLimit:
    enabled: true
    windowMs: 60000
    maxRequests: 100

# Логирование
logging:
  level: info          # debug | info | warn | error
  format: pretty       # json | pretty

# Модель LLM
model:
  provider: openrouter
  defaultModel: "anthropic/claude-sonnet-4-20250514"
  fallbackModel: "openai/gpt-4o"
  maxTokens: 8192
  temperature: 0.7

# Включение/отключение фич (опциональное, может быть в каждой фиче)
features:
  webchat:
    enabled: true
    port: 3001
    maxConnections: 1000
  voice:
    enabled: false
  container-sandbox:
    enabled: true   # После исправлений безопасности

# Память
memory:
  primary: sqlite    # sqlite | supabase | markdown
  path: ./data/memory.db
  selfEvolving: true
  compressionThreshold: 10000

# Безопасность
security:
  policy: config/security-policy.yaml
  secrets: encrypted
  auditLog: true
  allowlistMode: permissive   # permissive | strict

# Каналы коммуникации
channels:
  telegram:
    enabled: true
  webchat:
    enabled: true
```

**Важно:** Конфигурация горячо перезагружается — изменения применяются без перезапуска сервера.

#### Переменные окружения (`.env`)

Переменные окружения имеют приоритет над YAML и используются для секретов:

```bash
# Telegram (обязательно для Telegram-канала)
AGCLAW_TELEGRAM_TOKEN=123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11

# LLM провайдеры (хотя бы один)
OPENROUTER_API_KEY=sk-or-v1-xxxxx
# ANTHROPIC_API_KEY=sk-ant-xxxxx
# GEMINI_API_KEY=xxxxx

# Голос (опционально)
ELEVENLABS_API_KEY=sk_xxxxx

# Supabase (опционально, для облачной памяти)
SUPABASE_URL=https://xxxxx.supabase.co
SUPABASE_ANON_KEY=your_supabase_anon_key_here

# Мастер-ключ для зашифрованных секретов (автогенерируется если не задан)
# AGCLAW_MASTER_KEY=xxxxx
```

Полный список переменных смотрите в `.env.example`.

---

## 2. Архитектура

### 2.1 Обзор

AG-Claw расширяет OpenClaw модульной плагин-системой, мультиканальностью и многослойной безопасностью.

```
┌─────────────────────────────────────────────────────────────┐
│                      AG-Claw Framework                       │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │                  Core Layer                            │  │
│  │  ┌──────────┐  ┌──────────────┐  ┌─────────────────┐  │  │
│  │  │  Config   │  │ Plugin Loader│  │  Security Layer │  │  │
│  │  │  Manager  │  │              │  │  (NemoClaw)     │  │  │
│  │  │  (YAML +  │  │  Dynamic     │  │  ┌───────────┐  │  │  │
│  │  │   Env)    │  │  Feature     │  │  │ Allowlist │  │  │  │
│  │  │           │  │  Loading     │  │  │ Engine    │  │  │  │
│  │  │  Hot      │  │  + Lifecycle │  │  ├───────────┤  │  │  │
│  │  │  Reload   │  │  + Dep       │  │  │ Encrypted │  │  │  │
│  │  │           │  │  Resolution  │  │  │ Secrets   │  │  │  │
│  │  │           │  │              │  │  ├───────────┤  │  │  │
│  │  │           │  │              │  │  │ Policy    │  │  │  │
│  │  └──────────┘  └──────────────┘  │  │ Engine    │  │  │  │
│  │                                   └───────────┘  │  │  │
│  └────────────────────────────────────────────────────────┘  │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │                Feature Registry                        │  │
│  │  Features are self-contained modules with:             │  │
│  │  - Metadata (name, version, dependencies)              │  │
│  │  - Lifecycle hooks (init, start, stop, healthCheck)    │  │
│  │  - Own configuration from YAML                         │  │
│  │  - Access to shared context (logger, hooks, emit)      │  │
│  │                                                        │  │
│  │  ┌──────────────┐  ┌──────────────┐  ┌─────────────┐  │  │
│  │  │ Webchat      │  │ Voice        │  │ Knowledge   │  │  │
│  │  │ Browser Auto │  │ Workflows    │  │ Graph       │  │  │
│  │  │ Webhooks     │  │ Live Canvas  │  │ Morning     │  │  │
│  │  │ Container SB │  │ Air-Gapped   │  │ Briefing    │  │  │
│  │  │ ...26 total  │  │              │  │ ...         │  │  │
│  │  └──────────────┘  └──────────────┘  └─────────────┘  │  │
│  └────────────────────────────────────────────────────────┘  │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │               Channel Layer                            │  │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐             │  │
│  │  │ Telegram │  │ Webchat  │  │ Mobile   │  ...        │  │
│  │  │ (Bot API)│  │ (WS)     │  │ (Push)   │             │  │
│  │  └──────────┘  └──────────┘  └──────────┘             │  │
│  └────────────────────────────────────────────────────────┘  │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │               Memory Layer                             │  │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────────────────┐ │  │
│  │  │ SQLite   │  │ Markdown │  │ Supabase (pgvector)  │ │  │
│  │  │ Local DB │  │ Git-fri- │  │ Cloud-hosted with    │ │  │
│  │  │ + FTS5   │  │ endly    │  │ semantic search      │ │  │
│  │  └──────────┘  └──────────┘  └──────────────────────┘ │  │
│  │                                                        │  │
│  │  ┌──────────────────────────────────────────────────┐  │  │
│  │  │ Self-Evolving Memory Layer                       │  │  │
│  │  │ Consolidation · Pattern Discovery · Decay        │  │  │
│  │  └──────────────────────────────────────────────────┘  │  │
│  └────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────┘
```

### 2.2 Plugin System

Каждая фича — это модуль, реализующий интерфейс `FeatureModule`:

```typescript
interface FeatureModule {
  readonly meta: FeatureMeta;           // name, version, description, dependencies
  init(config, context): Promise<void>; // Вызывается при загрузке
  start?(): Promise<void>;             // Вызывается при включении
  stop?(): Promise<void>;              // Вызывается при выключении (очистка)
  healthCheck?(): Promise<HealthStatus>; // Периодическая проверка здоровья
}
```

#### Жизненный цикл

```
unloaded -> loading -> active -> disabled
                 \-> error
```

1. **Scan** — `PluginLoader` сканирует `src/features/` на наличие `index.ts`
2. **Register** — Каждая фича регистрируется с метаданными
3. **Resolve** — Зависимости разрешаются топологической сортировкой
4. **Init** — Фичам передаётся их YAML-конфиг и общий контекст
5. **Start** — Вызывается хук `start()` фичи
6. **Health** — Периодические проверки каждые 60 секунд
7. **Stop** — При остановке вызывается `stop()` в обратном порядке

#### Общий контекст

Фичам передаётся контекст при инициализации:

```typescript
interface FeatureContext {
  logger: Logger;                              // Логгер области видимости фичи
  config: AGClawConfig;                        // Полный конфиг
  registerHook(event, handler): void;          // Подписка на события
  emit(event, data): Promise<void>;            // Отправка событий другим фичам
}
```

Это позволяет фичам общаться без прямых импортов — слабые связи.

#### Добавление новой фичи

1. Создайте `src/features/my-feature/index.ts`
2. Экспортируйте `FeatureModule` по умолчанию
3. Добавьте конфиг в `config/default.yaml`:
   ```yaml
   features:
     my-feature:
       enabled: true
       customOption: value
   ```
4. Перезапустите AG-Claw. Плагин-лоудер подхватит автоматически.

---

## 3. Справочник по фичам

### Таблица всех фич (26 реализованных)

| # | Название | Описание | Статус | Зависимости |
|---|----------|----------|--------|---|
| 1 | `air-gapped` | Офлайн-режим с локальными моделями | ✅ | — |
| 2 | `auto-capture` | Автоматический захват решений/ошибок из сообщений | ✅ | — |
| 3 | `browser-automation` | Управление браузерами для скрапинга и автоматизации | ✅ | — |
| 4 | `budget` | Трекинг бюджета и лимитов | ✅ | — |
| 5 | `checkpoint` | Сейфы и восстановление задач | ✅ | — |
| 6 | `company-templates` | Шаблоны для бизнес-процессов | ✅ | — |
| 7 | `consolidation` | Консолидация памяти (слияние похожих записей) | ✅ | self-evolving |
| 8 | **`container-sandbox`** | Запуск кода в изолированных Docker-контейнерах | ✅ | — |
| 9 | `evening-recap` | Вечерние сводки дня | ✅ | — |
| 10 | `goal-decomposition` | Декомпозиция целей на подзадачи | ✅ | — |
| 11 | `goals` | Управление целями и трекинг прогресса | ✅ | — |
| 12 | `governance` | Управление изменениями и approval workflow | ✅ | — |
| 13 | `group-management` | Управление групповыми чатами | ✅ | — |
| 14 | `knowledge-graph` | Граф знаний с связями сущностей | ✅ | — |
| 15 | `life-domains` | Классификация жизненных сфер (работа, здоровье) | ✅ | — |
| 16 | `live-canvas` | Интерактивная доска для совместной работы | ✅ | — |
| 17 | **`mesh-workflows`** | Цепочки workflow с ветвлением и параллельностью | ✅ | checkpoint |
| 18 | `morning-briefing` | Утренние брифинги (календарь, погода, новости) | ✅ | — |
| 19 | `multimodal-memory` | Память для изображений, аудио, документов | ✅ | — |
| 20 | `skills-library` | Библиотека загружаемых навыков | ✅ | — |
| 21 | `smart-recommendations` | Контекстные рекомендации на основе паттернов | ✅ | — |
| 22 | `task-checkout` | Система checkout для задач | ✅ | — |
| 23 | `voice` | TTS/STT через ElevenLabs и Whisper | ✅ | — |
| 24 | **`webchat`** | Веб-чат с WebSocket, Markdown, загрузкой файлов | ✅ | — |
| 25 | **`webhooks`** | Приём и отправка webhook-событий | ✅ | — |
| 26 | `self-evolving` | Саморазвивающаяся память (консолидация, паттерны) | ✅ | — |

> **Примечание:** 4 выделенных фичи (`mesh-workflows`, `container-sandbox`, `webchat`, `webhooks`) были недавно исправлены с точки зрения безопасности.

### Детали по реализованным фичам

#### Webchat (веб-чат)

**Конфигурация:**
```yaml
features:
  webchat:
    enabled: true
    port: 3001
    maxConnections: 1000
    maxMessageHistory: 500
    maxFileSize: 10485760  # 10 MB
    allowedFileTypes: ["image/*", "text/*", "application/pdf"]
    uploadDir: ./uploads
```

**API:**
- `GET /` — HTML-интерфейс чата
- `GET /files/:id` — загрузка файла
- `WS /ws?room=default&user=user123&token=xxx` — WebSocket соединение

**WebSocket события:**
- `client → server`: `{type: "chat", content: "текст"}`
- `client → server`: `{type: "file", filename: "photo.jpg", mimeType: "image/jpeg", data: "base64..."}`
- `client → server`: `{type: "typing"}` — индикатор набора
- `server → client`: `{type: "message", message: {id, userId, content, role, timestamp}}`
- `server → client`: `{type: "typing", userId: "user123"}`
- `server → client`: `{type: "history", messages: [...]}`

#### Container Sandbox (песочница)

**Конфигурация:**
```yaml
features:
  container-sandbox:
    enabled: true

sandbox:
  enabled: true
  image: node:20-alpine
  memoryLimit: "512m"
  cpuLimit: "1.0"
  timeoutMs: 30000
  networkAccess: false      # без сети
  readOnlyRoot: true        # только чтение
  tmpfsSize: "64m"
```

**Публичный API (для других фич):**
```typescript
import containerSandbox from './features/container-sandbox';

const result = await containerSandbox.execute('ls -la', {
  timeoutMs: 10000,
  memoryLimit: '256m',
});

console.log(result.stdout);  // вывод
console.log(result.exitCode); // код выхода
console.log(result.timedOut); // таймаут?
```

**Безопасность:**
- Команды проходят через белый список разрешённых (`ls`, `cat`, `grep`, `node`, `python3` и др.)
- Нет shell-интерпретации (`sh -c`), аргументы передаются напрямую в `docker run`
- Изоляция сети, только tmpfs, read-only корень

#### Mesh Workflows (воркфлоу)

**Конфигурация:**
```yaml
features:
  mesh-workflows:
    enabled: true
    maxConcurrent: 10
    defaultTimeout: 300000  # 5 минут
    persistState: true
    checkpointDir: ./data/checkpoints
```

**API:**
```typescript
import meshWorkflows from './features/mesh-workflows';

// 1. Декомпозиция цели
const goal = "Создать MVP e-commerce магазина";
const decomposed = await meshWorkflows.decomposeGoal(goal);
// Результат:
// {
//   goal: "...",
//   subtasks: [
//     {id: 'research', dependencies: []},
//     {id: 'plan', dependencies: ['research']},
//     {id: 'execute', dependencies: ['plan']},
//     {id: 'verify', dependencies: ['execute']}
//   ],
//   executionPlan: ['research', 'plan', 'execute', 'verify']
// }

// 2. Создание workflow из декомпозиции
const workflow = meshWorkflows.createWorkflowFromGoal('goal-123', decomposed);

// 3. Запуск выполнения
const execution = await meshWorkflows.execute('goal-123', {
  extraVar: 'value'
});

// 4. Мониторинг прогресса
meshWorkflows.onProgress((exec) => {
  console.log(`Progress: ${exec.progress}%, step: ${exec.currentStep}`);
});

// 5. Получить статус
const status = meshWorkflows.getExecution(execution.id);
console.log(status.status); // running | completed | failed | paused
```

**Безопасность:**
- Условия в шагах `condition` оцениваются через `jsep` — безопасный парсер выражений без `eval`
- Никакого `new Function` или `eval`
- Шаги `agent` выполняются через безопасные хендлеры

#### Webhooks (входящие/исходящие)

**Конфигурация:**
```yaml
features:
  webhooks:
    enabled: true
    port: 3002
    path: /webhooks
    secret: ""          # секрет для подписи исходящих
    maxRetries: 3
    retryDelayMs: 1000
```

**API:**
```typescript
import webhooks from './features/webhooks';

// Регистрация обработчика события
webhooks.on('user.created', async (event) => {
  console.log('Новый пользователь:', event.payload);
  // Отправка уведомления в Slack
  await fetch('https://hooks.slack.com/...', {
    method: 'POST',
    body: JSON.stringify({ text: `User: ${event.payload.email}` }),
  });
});

// Создание подписки на исходящие вебхуки
const sub = webhooks.subscribe(
  'https://my.api/webhook',
  ['user.created', 'order.paid'],
  'shared-secret-key'
);

// Ручная отправка события
await webhooks.dispatch({
  id: 'evt_123',
  source: 'ag-claw',
  type: 'user.created',
  payload: { userId: 42, email: 'user@example.com' },
  headers: {},
  timestamp: Date.now(),
});
```

**Безопасность (SSRF защита):**
```typescript
// Проверка на внутренние хосты
private isInternalHostname(host: string): boolean {
  if (host === 'localhost' || host === '::1') return true;
  if (host.endsWith('.local')) return true;
  // Проверка приватных IP-диапазонов: 10.x, 192.168.x, 172.16-31.x, 169.254.x (AWS metadata)
  // ...
  return false;
}

// Валидация URL перед отправкой
private validateUrl(u: string): boolean {
  const parsed = new URL(u);
  if (this.isInternalHostname(parsed.hostname)) return false; // блокируем
  return true;
}
```

---

## 4. Безопасность

### 4.1 Обновлённые guidelines после исправлений

Недавно были устранены критические уязвимости в четырёх фичах:

| Фича | Проблема | Исправление |
|---|---|---|
| `mesh-workflows` | `new Function` для условий → RCE | Заменён на `jsep` + безопасный интерпретатор |
| `container-sandbox` | Shell injection, отсутствие белого списка | Белый список команд, прямое выполнение без `sh -c` |
| `webchat` | Отсутствие аутентификации, XSS | Bearer token в заголовках, ограничение по сети |
| `webhooks` | SSRF при отправке на внутренние адреса | Блокировка localhost, 169.254.169.254, приватных IP |

### 4.2 Как работает jsep парсер в mesh-workflows

Файл: `src/features/mesh-workflows/index.ts`

```typescript
import jsep from 'jsep';

function evaluateCondition(condition: string, vars: Record<string, unknown>): boolean {
  if (!condition || typeof condition !== 'string') return false;
  const ast = jsep(condition);  // Парсим выражение в AST
  const val = evalNode(ast, vars); // Безопасная оценка (только бинарные операции, идентификаторы, литералы)
  return !!val;
}
```

Поддерживаются операторы: `+ - * / > < >= <= == != === !== && !`. Никаких вызовов функций, свойств объекта (только member access), `with` или `eval`. Это безопасно для user-supplied условий.

### 4.3 Whitelist команд в container-sandbox

Файл: `src/features/container-sandbox/index.ts`

```typescript
const whitelist = new Set([
  'ls','cat','echo','grep','find','node','python','npm','curl','wget',
  'stat','du','df','ps','whoami','id','head','tail','jq','sed','awk',
]);

// Проверка базового имени команды
if (!whitelist.has(base)) {
  throw new Error('Command not allowed');
}

// Docker args собираются без shell, и строки не передаются через sh -c
const args = ['run', '--rm', '--name', containerName, /* ... */];
args.push(config.image);
for (const p of parts) args.push(p);  // argv напрямую
```

Запрещённые команды (например, `rm`, `sudo`, `mkfs`) — не в белом списке.

### 4.4 Токен-аутентификация в webchat

В `src/features/webchat/index.ts` добавлен опциональный `authToken`:

```typescript
private authToken: string | null = null;

async init(config, context) {
  this.authToken = (config as any).authToken ?? null;
}

// Проверка для HTTP endpoints
if (this.authToken) {
  const authHeader = req.headers['authorization'];
  if (!authHeader || authHeader !== `Bearer ${this.authToken}`) {
    res.writeHead(401); res.end('Unauthorized'); return;
  }
}

// Проверка для WebSocket
const token = parsedUrl.searchParams.get('token');
if (this.authToken && token !== this.authToken) {
  ws.close(4001, 'Unauthorized');
  return;
}
```

**Использование:**
```yaml
features:
  webchat:
    authToken: "your-secret-token-here"
```
Теперь клиент должен подключаться с `?token=your-secret-token-here`.

### 4.5 SSRF защита в webhooks

В `src/features/webhooks/index.ts`:

```typescript
private isInternalHostname(host: string): boolean {
  const h = host.toLowerCase();
  if (h === 'localhost' || h === '::1') return true;
  if (h.endsWith('.local')) return true;
  // Проверка IPv4
  const parts = h.split('.');
  if (parts.length === 4) {
    const [a,b] = parts.map(Number);
    if (a === 127) return true;           // 127.0.0.0/8
    if (a === 10) return true;           // 10.0.0.0/8
    if (a === 192 && b === 168) return true; // 192.168.0.0/16
    if (a === 172 && b >= 16 && b <= 31) return true; // 172.16.0.0/12
    if (a === 169 && b === 254) return true; // 169.254.169.254 (AWS metadata)
  }
  return false;
}

private validateUrl(u: string): boolean {
  try {
    const parsed = new URL(u);
    if (this.isInternalHostname(parsed.hostname)) return false;
    return true;
  } catch { return false; }
}
```

Прежде чем отправить webhook, вызывается `validateUrl(sub.url)`. Если URL ведёт на internal host — отправка блокируется и логируется.

---

## 5. API Reference

### 5.1 REST API Endpoints

AG-Claw предоставляет несколько HTTP-эндпоинтов (в основном от фичей):

| Метод | Путь | Описание | Фича |
|---|---|---|---|
| `GET` | `/` | HTML-интерфейс webchat | webchat |
| `GET` | `/files/:id` | Загрузка файла по ID | webchat |
| `GET` | `/health` | Health check всех активных фич | core |
| `POST` | `/webhooks` | Приём входящих webhook-событий | webhooks |
| `GET` | `/api/status` | Статус агента (требуется аутентификация) | core |

`/health` ответ:
```json
{
  "status": "ok",
  "timestamp": 1742301234567,
  "features": {
    "webchat": {"healthy": true, "details": {"clients": 2}},
    "container-sandbox": {"healthy": true},
    "mesh-workflows": {"healthy": true}
  }
}
```

### 5.2 WebSocket Events

**WebSocket сервер** запускается фичей `webchat` на порту `3001` (по умолчанию).

**Подключение:**
```
ws://localhost:3001/ws?room=default&user=user123
# Или с токеном (если включена аутентификация)
ws://localhost:3001/ws?room=default&user=user123&token=xxx
```

**События от клиента:**

| Событие | Формат | Описание |
|---|---|---|
| `chat` | `{type: "chat", content: "текст"}` | Отправка текстового сообщения |
| `file` | `{type: "file", filename: "photo.jpg", mimeType: "image/jpeg", data: "base64"}` | Загрузка файла |
| `typing` | `{type: "typing"}` | Индикатор набора |

**События от сервера:**

| Событие | Формат | Описание |
|---|---|---|
| `message` | `{type: "message", message: {id, userId, roomId, content, role, timestamp}}` | Новое сообщение |
| `typing` | `{type: "typing", userId: "user123"}` | Кто-то печатает |
| `history` | `{type: "history", messages: [...]}` | История чата при подключении |
| `error` | `{type: "error", message: "текст"}` | Ошибка |

**Пример обработки на клиенте:**
```javascript
const ws = new WebSocket('ws://localhost:3001/ws?room=default&user=me');

ws.onmessage = (event) => {
  const data = JSON.parse(event.data);
  if (data.type === 'message') {
    console.log(`${data.message.role}: ${data.message.content}`);
  }
  if (data.type === 'typing') {
    console.log(`${data.userId} печатает...`);
  }
};

// Отправка сообщения
ws.send(JSON.stringify({ type: 'chat', content: 'Привет!' }));
```

### 5.3 Webhook Payload Format

При отправке webhook-события подписчикам используется следующий payload:

```json
{
  "id": "evt_1234567890",
  "source": "ag-claw",
  "type": "user.created",
  "payload": {
    "userId": 42,
    "email": "user@example.com",
    "createdAt": 1742301234567
  },
  "headers": {
    "user-agent": "AG-Claw/0.2.0"
  },
  "timestamp": 1742301234567,
  "signature": "sha256=abc123def456..."  // HMAC-SHA256 от тела JSON с secret подписчика
}
```

**Заголовки при отправке:**
- `Content-Type: application/json`
- `X-Webhook-Signature: sha256=<digest>` — для верификации на стороне получателя
- `X-Webhook-Event: user.created` — тип события
- `X-Webhook-Id: evt_1234567890` — уникальный ID

**Подпись (на стороне отправителя):**
```typescript
const crypto = require('crypto');
const signature = crypto.createHmac('sha256', subscriberSecret)
                        .update(JSON.stringify(event))
                        .digest('hex');
// Отправляем: `X-Webhook-Signature: sha256=${signature}`
```

**Проверка на стороне получателя:**
```typescript
const expected = crypto.createHmac('sha256', secret)
                       .update(payload)
                       .digest('hex');
const provided = signature.replace('sha256=', '');
if (expected !== provided) throw new Error('Invalid signature');
```

---

## 6. Разработка и тестирование

### 6.1 Структура проекта

```
ag-claw/
├── src/
│   ├── index.ts              # Main entry (Agent, AGClaw class)
│   ├── core/
│   │   ├── config.ts         # Config manager (YAML + env, Zod validation)
│   │   ├── plugin-loader.ts  # Feature loading, lifecycle, dependency resolution
│   │   ├── logger.ts         # Structured logging (pino)
│   │   └── llm-provider.ts   # LLM abstraction (OpenRouter, OpenAI, Anthropic)
│   ├── features/             # 26 feature modules
│   │   ├── webchat/
│   │   ├── container-sandbox/
│   │   ├── mesh-workflows/
│   │   └── ...
│   ├── channels/             # Channel adapters (telegram.ts uses grammy)
│   ├── memory/               # Memory backends (sqlite, supabase, markdown, graph, semantic)
│   └── security/             # Security layer (encrypted-secrets, allowlists, policy-engine)
├── config/
│   ├── default.yaml          # Main configuration
│   └── security-policy.yaml  # Security policy (allowlists, rate limits, sandbox)
├── docs/
│   ├── architecture.md
│   ├── FEATURES.md
│   ├── installation.md
│   └── developer-guide.md    # Этот файл
├── tests/                    # Jest tests
├── docker/
│   ├── Dockerfile
│   └── docker-compose.yml
├── .env.example
├── package.json
├── tsconfig.json
└── README.md
```

### 6.2 Запуск тестов

```bash
# Unit tests
npm test

# With coverage
npm test -- --coverage

# E2E tests (если есть e2e/)
npm run test:e2e
```

### 6.3 Линтинг

```bash
npm run lint
# Автофикс
npm run lint -- --fix
```

### 6.4 Сборка

```bash
# TypeScript → JavaScript
npm run build

# В development режиме с hot-reload
npm run dev
```

### 6.5 Отладка

Логи пишутся через `pino`. Формат определяется в конфиге (`pretty` для dev, `json` для prod).

```typescript
import { createLogger } from './core/logger';
const log = createLogger().child({ feature: 'my-feature' });

log.info('Started', { userId: 123, timeout: 5000 });
log.warn('High memory usage', { rss: process.memoryUsage().rss });
log.error('Failed to connect', { error: err.message });
```

Log-level контролируется через `logging.level` в YAML или `AGCLAW_LOG_LEVEL`.

---

## 7. Contributing

1. Fork репозитория
2. Создайте feature branch (`git checkout -b feature/my-feature`)
3. Добавьте фичу в `src/features/` (соблюдайте интерфейс `FeatureModule`)
4. Обновите документацию (`docs/FEATURES.md` и эту)
5. Запустите линтер и тесты
6. Создайте Pull Request с описанием изменений

**Правила:**
- Никогда не используйте `eval`, `new Function`, `child_process.exec` без санитайзера
- Все пользовательские входы должны валидироваться через Zod схемы
- Чувствительные данные (API keys) — только через `.env`, никогда не коммитьте
- Для network-запросов используйте `fetch` с таймаутами и обработкой ошибок
- Файлы в `security-audit-features.md` содержат рекомендации — следуйте им

---

## 8. Приложение: Все фичи подрробно

### 8.1 Air-Gapped (офлайн-режим)

**Конфиг:**
```yaml
features:
  air-gapped:
    enabled: true
```

**Behaviour:** Запрещает любые внешние network-запросы. Использует только локальные модели (например, Ollama). LLM провайдер должен быть настроен на `ollama` или `local`.

### 8.2 Auto-Capture (автозахват)

Автоматически извлекает решения, уроки и ошибки из текстовых сообщений:

```typescript
import autoCapture from './features/auto-capture';

const captures = autoCapture.detectCaptures(
  "Сегодня я learn that нужно коммитить чаще. Ошибка: забыл запушить код.",
  "telegram:12345"
);
// captures = [
//   { type: 'lesson', content: 'нужно коммитить чаще', confidence: 0.9 },
//   { type: 'error', content: 'забыл запушить код', confidence: 0.8 }
// ]
```

### 8.3 Knowledge Graph (граф знаний)

**Backend:** SQLite (по умолчанию) или Neo4j.

```typescript
import knowledgeGraph from './features/knowledge-graph';

// Добавить сущность
await knowledgeGraph.addNode({
  id: 'person:alice',
  type: 'person',
  properties: { name: 'Alice', role: 'developer' }
});

// Добавить связь
await knowledgeGraph.addEdge('person:alice', 'works_at', 'company:acme');

// Запрос
const results = await knowledgeGraph.query(`
  MATCH (p:person)-[r:works_at]->(c)
  WHERE p.name = 'Alice'
  RETURN c.name
`);
```

---

*AG-Claw v0.2.0 — Documentation last updated: 2026-03-17*
