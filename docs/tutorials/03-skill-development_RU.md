# Урок 3: Разработка навыков

*Примерное время: 30 минут*

В этом уроке вы создадите собственный навык, который добавит новые возможности вашему агенту. Навыки (skills) — это основной способ расширения AG-Claw: они регистрируют инструменты, которые агент может вызывать во время разговоров.

---

## Что такое навык?

Навык — это модуль-функция, который регистрирует один или несколько **инструментов** у агента. Инструменты — это функции, которые агент может вызывать при обработке разговора. Когда агент решает, что инструмент поможет ответить на ваш вопрос, он вызывает инструмент и включает результат в свой ответ.

Примеры встроенных инструментов:
- `web_search` — поиск в интернете
- `memory_search` — поиск в семантической памяти
- `run_command` — выполнение shell-команды
- `read_file` / `write_file` — файловые операции

---

## Проект: создание навыка "Git-ассистент"

Мы создадим навык, который поможет агенту взаимодействовать с Git-репозиториями. Навык зарегистрирует инструменты для:

- `git_status` — показать текущий статус git
- `git_log` — показать недавние коммиты
- `git_branch` — список всех веток

### Шаг 1 — Создайте директорию навыка

```bash
mkdir -p src/features/git-assistant
```

### Шаг 2 — Реализуйте модуль навыка

Создайте `src/features/git-assistant/index.ts`:

```typescript
import { FeatureModule, FeatureMeta, FeatureContext, HealthStatus } from '../../core/types';
import { execSync } from 'child_process';
import { existsSync } from 'fs';

const meta: FeatureMeta = {
  name: 'git-assistant',
  version: '0.0.2',
  description: 'Git repository assistant — status, log, branches, and more',
  dependencies: [],
};

class GitAssistant implements FeatureModule {
  readonly meta = meta;
  private ctx!: FeatureContext;
  private repoPath: string = process.cwd();

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.repoPath = (config.repoPath as string) ?? process.cwd();
    this.ctx.logger.info('GitAssistant initialized', { repoPath: this.repoPath });
  }

  async start(): Promise<void> {
    // Verify this is a git repo
    if (!existsSync(`${this.repoPath}/.git`)) {
      this.ctx.logger.warn('Not a git repository', { path: this.repoPath });
    }
    this.ctx.logger.info('GitAssistant started');
  }

  async stop(): Promise<void> {
    this.ctx.logger.info('GitAssistant stopped');
  }

  async healthCheck(): Promise<HealthStatus> {
    const isRepo = existsSync(`${this.repoPath}/.git`);
    return {
      healthy: true,
      message: isRepo ? `Git repo at ${this.repoPath}` : `Not a git repo: ${this.repoPath}`,
    };
  }

  // Tool: git_status
  private async gitStatus(): Promise<string> {
    try {
      const output = execSync('git status --short', {
        cwd: this.repoPath,
        encoding: 'utf-8',
        maxBuffer: 1024 * 1024,
      });
      return output.trim() || 'Working tree clean';
    } catch (err: unknown) {
      const e = err as { message: string };
      return `Git error: ${e.message}`;
    }
  }

  // Tool: git_log
  private async gitLog(limit: number = 10): Promise<string> {
    try {
      const output = execSync(`git log --oneline -n ${limit}`, {
        cwd: this.repoPath,
        encoding: 'utf-8',
        maxBuffer: 1024 * 1024,
      });
      return output.trim();
    } catch (err: unknown) {
      const e = err as { message: string };
      return `Git error: ${e.message}`;
    }
  }

  // Tool: git_branch
  private async gitBranch(): Promise<string> {
    try {
      const output = execSync('git branch -a', {
        cwd: this.repoPath,
        encoding: 'utf-8',
        maxBuffer: 1024 * 1024,
      });
      return output.trim();
    } catch (err: unknown) {
      const e = err as { message: string };
      return `Git error: ${e.message}`;
    }
  }

  getTools() {
    return [
      {
        name: 'git_status',
        description: 'Show the current git repository status (short format). Returns modified, added, and deleted files.',
        parameters: {},
        execute: async () => this.gitStatus(),
      },
      {
        name: 'git_log',
        description: 'Show recent git commit history.',
        parameters: {
          limit: { type: 'number', description: 'Number of commits to show (default: 10)', required: false },
        },
        execute: async (params) => this.gitLog((params.limit as number) ?? 10),
      },
      {
        name: 'git_branch',
        description: 'List all local and remote git branches. Current branch is marked with an asterisk.',
        parameters: {},
        execute: async () => this.gitBranch(),
      },
    ];
  }
}

export default new GitAssistant();
```

### Шаг 3 — Зарегистрируйте инструменты у агента

Навыку нужно зарегистрировать свои инструменты у агента. Обновите метод `init()` вашей функции:

```typescript
async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
  this.ctx = context;

  // Register tools with the agent
  for (const tool of this.getTools()) {
    this.ctx.registerTool(tool);
  }

  this.ctx.logger.info('GitAssistant initialized', { repoPath: this.repoPath });
}
```

Точный API для `registerTool` зависит от интерфейса FeatureContext. Если в вашем контексте нет `registerTool`, навык может вместо этого испустить hook:

```typescript
// Alternative: emit a hook
await context.emit('skill:register', {
  name: this.meta.name,
  tools: this.getTools(),
});
```

### Шаг 4 — Добавьте конфигурацию

Обновите `config/default.yaml`:

```yaml
features:
  git-assistant:
    enabled: false
    repoPath: "."   # Path to the git repository
```

### Шаг 5 — Включите и протестируйте

```bash
# Rebuild после добавления функции
npm run build

# Включите функцию
agclaw feature git-assistant enable

# Перезапустите шлюз
agclaw gateway restart

# Тест через чат
curl -X POST http://localhost:3000/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "What is our git status?", "userId": "test"}'
```

Ожидаемый ответ:
```
Based on the git status:
 M README.md
?? docs/
```

---

## Анатомия инструмента

Каждый инструмент в навыке состоит из четырёх частей:

```typescript
{
  name: 'tool_name',           // Уникальный идентификатор (используется в промптах LLM)
  description: 'What it does',  // Описание для LLM (будьте ясны!)
  parameters: {                 // JSON Schema для параметров
    type: 'object',
    properties: {
      input: {
        type: 'string',
        description: 'What to process',
        required: true          // Обязательный vs опциональный
      },
      limit: {
        type: 'number',
        description: 'Max items',
        required: false
      }
    },
    required: ['input']         // Список обязательных параметров
  },
  execute: async (params) => {  // actual implementation
    const input = params.input as string;
    const result = doSomething(input);
    return JSON.stringify(result);  // Must return a string
  }
}
```

### Написание хороших описаний инструментов

Описание критически важно — оно говорит LLM, когда использовать инструмент. Будьте конкретны:

```typescript
// Bad — слишком расплывчато
description: 'Search something'

// Good — конкретно
description: 'Search the web for information. Returns titles and snippets from search results.'

// Good — объясняет формат ввода
description: 'Search git history. Input should be a grep pattern to match against commit messages.'

// Good — объясняет формат вывода
description: 'List directory contents. Returns a table with columns: name, size, modified.'
```

### Обработка ошибок в инструментах

Инструменты должны возвращать сообщения об ошибках как строки, а не бросать исключения:

```typescript
execute: async (params) => {
  try {
    const result = riskyOperation(params.input);
    return JSON.stringify(result);
  } catch (err) {
    // Return error as string — the agent can then explain it to the user
    return `Error: ${err instanceof Error ? err.message : String(err)}`;
  }
}
```

---

## Передача контекста в инструменты

Инструментам часто нужен доступ к большему, чем просто их параметры. Навык может хранить контекст в `init()`:

```typescript
class MySkill implements FeatureModule {
  private ctx!: FeatureContext;
  private db!: Database;

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.db = new Database(config.dbPath as string);
  }

  // Tool can access this.db
  execute: async (params) => {
    const results = this.db.query(params.sql as string);
    return JSON.stringify(results);
  }
}
```

---

## Лучшие практики для навыков

### 1. Делайте инструменты сфокусированными

Каждый инструмент должен хорошо делать одну вещь. Не создавайте `doEverything` инструмент:

```typescript
// Bad
{ name: 'filesystem', execute: async (params) => { /* does everything */ } }

// Good
{ name: 'read_file', execute: async (params) => { /* reads one file */ } }
{ name: 'write_file', execute: async (params) => { /* writes one file */ } }
{ name: 'list_directory', execute: async (params) => { /* lists one dir */ } }
```

### 2. Валидируйте параметры

```typescript
execute: async (params) => {
  if (!params.path || typeof params.path !== 'string') {
    return 'Error: path parameter is required and must be a string';
  }
  // Continue...
}
```

### 3. Ограничивайте размер вывода

Длинные выводы могут переполнить контекст LLM. Обрезайте большие результаты:

```typescript
execute: async (params) => {
  const result = hugeOperation();
  if (result.length > 5000) {
    return result.slice(0, 5000) + '\n... (truncated)';
  }
  return result;
}
```

### 4. Логируйте с умом

Логируйте вызовы инструментов для отладки, но никогда не логируйте значения параметров, которые могут содержать секреты:

```typescript
execute: async (params) => {
  this.ctx.logger.info('Tool called', { name: 'my_tool' });
  // Don't log: this.ctx.logger.info('Tool called', { apiKey: params.apiKey });

  const result = doWork(params);
  return result;
}
```

### 5. Всегда возвращайте строки

Исполнитель инструментов ожидает строковые возвраты. Конвертируйте всё в строки:

```typescript
execute: async (params) => {
  const num = someNumber;
  return String(num);                    // Good
  return JSON.stringify({ value: num }); // Good for structured data
  return num;                            // Bad — must be string
}
```

---

## Публикация вашего навыка

Когда навык заработает, подумайте о том, чтобы поделиться им:

1. Добавьте его в репозиторий AG-Claw через pull request
2. Документируйте его в библиотеке навыков (`skills-library` feature)
3. Напишите руководство по использованию

---

## Следующие шаги

- **[Урок 4: Развёртывание](./04-deployment_RU.md)** — Разверните агента с Docker
- **[Урок 5: Продвинутые паттерны](./05-advanced-patterns_RU.md)** — Мультиагентная координация, mesh-workflows

---

## Решение проблем

| Проблема | Решение |
|---|---|
| Инструмент не появляется | Пересоберите с `npm run build` и перезапустите шлюз |
| Агент не вызывает инструмент | Улучшите описание инструмента — LLM нужно понимать, когда его использовать |
| Инструмент всегда возвращает ошибку | Проверьте `agclaw gateway logs` на наличие реального исключения |
| Функция не загружается | Запустите `agclaw doctor` для диагностики проблем с зависимостями |

---

*Вопросы? Создайте issue на [GitHub](https://github.com/AG064/ag-claw/issues).*
