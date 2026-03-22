# Руководство пользователя

Полное руководство по эксплуатации AG-Claw в продакшене и повседневном использовании. Этот документ охватывает архитектуру, конфигурацию, управление памятью, безопасность, развёртывание и многое другое.

---

## Содержание

1. [Обзор архитектуры](#1-обзор-архитектуры)
2. [Создание и управление агентами](#2-создание-и-управление-агентами)
3. [Система памяти](#3-система-памяти)
4. [Навыки и как их использовать](#4-навыки-и-как-их-использовать)
5. [Лучшие практики безопасности](#5-лучшие-практики-безопасности)
6. [Варианты развёртывания](#6-варианты-развёртывания)
7. [Мониторинг и логирование](#7-мониторинг-и-логирование)
8. [Резервное копирование и восстановление](#8-резервное-копирование-и-восстановление)
9. [Справочник API](#9-справочник-api)
10. [Справочник конфигурации](#10-справочник-конфигурации)

---

## 1. Обзор архитектуры

### Философия дизайна

AG-Claw построен на трёх принципах:

1. **Модульность вместо монолита** — каждая возможность представляет собой модуль-функцию, который можно независимо включать или выключать
2. **Память как полноценный гражданин** — агент не просто обрабатывает сообщения; он запоминает, учится и развивается
3. **Безопасность по умолчанию** — шифрование, логирование аудита и применение политик встроены изначально

### Схема системы

```
┌──────────────────────────────────────────────────────────────────┐
│                        Ваша инфраструктура                        │
│                                                                  │
│   ┌─────────────────────────────────────────────────────────┐    │
│   │                    AG-Claw Gateway                      │    │
│   │                        :18789                            │    │
│   ├───────────────┬────────────────────┬────────────────────┤    │
│   │  Каналы       │   Модули функций   │   Ядро             │    │
│   │               │                    │                     │    │
│   │  • Telegram    │  • sqlite-memory  │  • Plugin Loader  │    │
│   │  • Discord     │  • semantic-search│  • LLM Provider    │    │
│   │  • Webchat     │  • cron-scheduler  │  • Config Manager  │    │
│   │  • Slack       │  • morning-brief   │  • Logger         │    │
│   │  • WhatsApp    │  • audit-log       │  • Security Layer │    │
│   │  • Email       │  • mesh-workflows  │                    │    │
│   │  • SMS         │  • ... (ещё 53)    │                    │    │
│   └───────┬───────┴────────┬──────────┴──────────┬──────────┘    │
│           │                 │                      │               │
│           │     ┌───────────▼────────────────────▼─────────┐     │
│           │     │        Agentic Tool Loop                │     │
│           │     │                                          │     │
│           │     │  Сообщение → LLM → Вызовы инструментов → Память │
│           │     │                                          │     │
│           │     │  ┌─────────────────────────────────────┐ │     │
│           │     │  │ Семантическая  │  Граф знаний     │ │     │
│           │     │  │ память         │  Чекпоинты        │ │     │
│           │     │  └─────────────────────────────────────┘ │     │
│           │     └──────────────────────────────────────────┘     │
│           │                                                       │
│   ┌───────▼────────┐                            ┌────────────────▼┐│
│   │   Конечные     │                            │  Внешние API   ││
│   │ пользователи   │                            │ OpenRouter,    ││
│   └────────────────┘                            │ Anthropic и др ││
│                                                  └────────────────┘│
└──────────────────────────────────────────────────────────────────┘
```

### Основные компоненты

| Компонент | Файл(ы) | Назначение |
|---|---|---|
| **Gateway** | `src/index.ts` | HTTP-сервер, загружает все функции, управляет жизненным циклом |
| **CLI** | `src/cli.ts` | Командный интерфейс для пользователя |
| **Plugin Loader** | `src/core/plugin-loader.ts` | Обнаруживает, загружает, запускает и проверяет работоспособность 59 модулей |
| **Agent** | `src/index.ts` (класс `Agent`) | Agentic Tool Loop — оркестрирует LLM + инструменты + память |
| **LLM Provider** | `src/core/llm-provider.ts` | Абстракция над OpenRouter, Anthropic, OpenAI |
| **Memory** | `src/memory/` | Семантический поиск, граф знаний, SQLite-хранилище |
| **Channels** | `src/channels/` | Адаптеры протоколов (Telegram, Discord, Webchat) |
| **Features** | `src/features/*/index.ts` | 59 отдельных модулей |
| **Security** | `src/security/` | Движок политик, зашифрованные секреты, списки доступа |

### Agentic Tool Loop

Когда пользователь отправляет сообщение, AG-Claw обрабатывает его через следующий цикл:

1. **Получение сообщения** — адаптер канала нормализует входные данные
2. **Проверка безопасности** — списки доступа, ограничение скорости, фильтрация контента
3. **Автозахват** — функция обнаруживает решения, уроки, ошибки в сообщении
4. **Вызов LLM** — сообщение отправляется модели с историей разговора и доступными инструментами
5. **Выполнение инструментов** — если модель вызывает инструмент, он выполняется, и результаты возвращаются
6. **Обновление памяти** — семантическая память сохраняет взаимодействие; граф знаний обновляется
7. **Ответ** — итоговый текст возвращается пользователю через адаптер канала

Цикл выполняется до 10 итераций на сообщение для обработки сложных многошаговых задач.

---

## 2. Создание и управление агентами

### Создание первого агента

После `agclaw init` ваш агент готов к использованию. Конфигурация по умолчанию создаёт универсального агента. Для настройки:

```bash
# Просмотр текущей конфигурации
agclaw config

# Установка имени агента
agclaw config name "Мой персональный ассистент"

# Установка системного промпта (через переменную окружения)
export AGCLAW_SYSTEM_PROMPT="Вы — опытный программист..."
```

### Управление несколькими агентами

AG-Claw поддерживает несколько профилей агентов. Создание нового агента:

```bash
# Создание именованного профиля агента
agclaw agents create --name "coding-assistant" \
  --system-prompt "Вы — эксперт-программист..." \
  --model "anthropic/claude-sonnet-4-20250514"
```

Список агентов:

```bash
agclaw agents list
```

Переключение между агентами:

```bash
agclaw agents use "coding-assistant"
```

### Конфигурация агента

Основные настройки агента в `agclaw.json`:

```json
{
  "agent": {
    "name": "AG-Claw Ассистент",
    "systemPrompt": "Вы — полезный AI-ассистент...",
    "maxIterations": 10,
    "temperature": 0.7,
    "tools": ["web_search", "read_file", "write_file", "run_command"]
  },
  "model": {
    "provider": "openrouter",
    "defaultModel": "anthropic/claude-sonnet-4-20250514",
    "fallbackModel": "openai/gpt-4o",
    "maxTokens": 8192,
    "temperature": 0.7,
    "retryAttempts": 3
  }
}
```

### Сессии разговоров

AG-Claw поддерживает историю разговоров для каждого пользователя:

```bash
# Просмотр недавних сессий
agclaw sessions list

# Просмотр конкретной сессии
agclaw sessions view <session-id>

# Очистка сессии
agclaw sessions clear <session-id>
```

Сессии хранятся в `data/sessions.db` и включают полную историю сообщений, вызовы инструментов и статистику использования LLM.

---

## 3. Система памяти

AG-Claw имеет многоуровневую архитектуру памяти. Каждый слой служит своей цели.

### Уровни памяти

```
┌────────────────────────────────────────────┐
│        Семантическая память                 │  ← Быстрый поиск по ключевым словам
│      (SQLite + модели эмбеддингов)          │     Используется каждую сессию
├────────────────────────────────────────────┤
│           Граф знаний                       │  ← Связи между сущностями
│      (узлы + рёбра, обход графа)            │     Используется для рассуждений
├────────────────────────────────────────────┤
│         Markdown-память                     │  ← Понятные человеку заметки
│         (файлы на диске, отслеживаемые)      │     Используется для долгосрочных фактов
├────────────────────────────────────────────┤
│        Саморазвивающаяся память             │  ← Автоматическая консолидация
│   (периодическое сжатие + абстракция)       │     Поддерживает эффективность
└────────────────────────────────────────────┘
```

### Семантическая память

Основная рабочая память. Хранит взаимодействия, уроки, решения.

```bash
# Сохранить запись вручную
agclaw memory store "decision" "Использовать PostgreSQL для основной базы данных"

# Поиск в памяти
agclaw memory search "решения по базе данных"

# Просмотр недавних записей
agclaw memory recent --limit 10

# Статистика памяти
agclaw memory stats
```

Через инструмент агента `memory_search`:

```
Пользователь: Какие решения мы приняли по дизайну API?
Агент: Сейчас проверю... [вызывает memory_search]
     Найдено 3 релевантные записи:
     [decision] Использовать REST вместо GraphQL для публичного API (доступ 5x)
     [decision] Версионировать все эндпоинты под префиксом /api/v1/ (доступ 3x)
     [decision] Использовать JWT токены с истечением 1 час (доступ 2x)
```

### Граф знаний

Хранит сущности и их связи. Позволяет рассуждать о связях.

```bash
# Просмотр статистики графа знаний
agclaw memory graph stats

# Экспорт графа
agclaw memory graph export > graph.json
```

### Сжатие памяти

Когда семантическая память превышает порог сжатия (по умолчанию: 10 000 записей), функция саморазвивающейся памяти автоматически:

1. Находит связанные записи
2. Консолидирует их в абстракции более высокого уровня
3. Удаляет избыточные записи
4. Сохраняет существенную информацию в сжатом виде

Настройка:

```json
{
  "features": {
    "self-evolving-memory": {
      "enabled": true,
      "compressionThreshold": 10000,
      "consolidationIntervalHours": 24
    }
  }
}
```

### Чекпоинты

Сохранение и возобновление долгих задач:

```bash
# Сохранить чекпоинт (через инструмент агента)
Агент: memory_checkpoint(taskId="build-2024-01", state={...})

# Возобновить
Агент: memory_resume(taskId="build-2024-01")
```

---

## 4. Навыки и как их использовать

Навыки (skills) — это многократно используемые пакеты возможностей, расширяющие то, что агент может делать. AG-Claw поставляется со встроенной библиотекой навыков.

### Встроенные навыки

| Навык | Что делает |
|---|---|
| `web_search` | Поиск в интернете через DuckDuckGo |
| `get_current_time` | Возвращает текущую дату/время |
| `read_file` | Читает файл с диска |
| `write_file` | Записывает контент в файл |
| `run_command` | Выполняет shell-команду |
| `memory_search` | Ищет в семантической памяти |
| `memory_store` | Сохраняет новую запись в памяти |
| `memory_checkpoint` | Сохраняет чекпоинт задачи |
| `memory_resume` | Возобновляет задачу из чекпоинта |

### Установка дополнительных навыков

Просмотр библиотеки навыков:

```bash
agclaw skills list
```

Установка навыка:

```bash
agclaw skills install <имя-навыка>
```

### Создание собственных навыков

Создайте `src/features/skills-library/<мой-навык>/index.ts`:

```typescript
import { SkillModule } from '../../types';

const mySkill: SkillModule = {
  name: 'my-skill',
  version: '0.1.0',
  description: 'Делает что-то полезное',

  tools: [{
    name: 'my_tool',
    description: 'Делает что-то полезное',
    parameters: {
      input: { type: 'string', required: true }
    },
    execute: async (params) => {
      return `Обработано: ${params.input}`;
    }
  }],

  init: async () => {},
  start: async () => {},
  stop: async () => {},
};

export default mySkill;
```

Зарегистрируйте его в `config/default.yaml` или `agclaw.json`:

```json
{
  "features": {
    "skills-library": {
      "enabled": true,
      "skills": ["my-skill"]
    }
  }
}
```

---

## 5. Лучшие практики безопасности

### Основные шаги безопасности

**1. Никогда не коммитьте API-ключи в git**

Используйте переменные окружения, а не захардкоженные токены:

```bash
# Правильно
export OPENROUTER_API_KEY=sk-or-v1-...

# Неправильно — попадёт на GitHub
echo '"token": "sk-or-v1-..."' >> agclaw.json
```

Добавьте `agclaw.json` в `.gitignore`:

```gitignore
agclaw.json
data/
*.db
.env
```

**2. Используйте зашифрованное хранилище секретов**

```bash
# Зашифровать секрет
agclaw secrets set OPENROUTER_API_KEY "sk-or-v1-..."
```

**3. Настраивайте списки доступа**

Ограничьте доступ к Telegram по конкретным ID пользователей:

```json
{
  "channels": {
    "telegram": {
      "allowedUsers": [123456789, 987654321]
    }
  }
}
```

**4. Включайте ограничение скорости**

```json
{
  "security": {
    "rateLimit": {
      "windowMs": 60000,
      "maxRequests": 30
    }
  }
}
```

**5. Регулярно просматривайте логи аудита**

```bash
agclaw audit log --last 24h
```

### Справочник функций безопасности

| Функция | Файл | Назначение |
|---|---|---|
| Зашифрованные секреты | `src/security/encrypted-secrets.ts` | AES-256 шифрование для API-ключей |
| Движок политик | `src/security/policy-engine.ts` | YAML-определённые политики безопасности |
| Списки доступа | `src/security/allowlists.ts` | Белые списки пользователей/чатов |
| Ограничение скорости | `src/features/rate-limiting/` | Потребление запросов на пользователя |
| Логи аудита | `src/features/audit-log/` | Неизменяемые записи вызовов инструментов |
| Фильтрация контента | `src/features/content-filtering/` | Санитизация входных данных |

---

## 6. Варианты развёртывания

### Локальная разработка

```bash
npm install
npm run build
npm start
```

### Docker (рекомендуется для продакшена)

```bash
# Сборка образа
npm run docker:build

# Запуск контейнеров
npm run docker:up

# Просмотр логов
docker compose -f docker/docker-compose.yml logs -f

# Остановка
npm run docker:down
```

### Конфигурация Docker Compose

Отредактируйте `docker/docker-compose.yml` для вашего окружения:

```yaml
services:
  ag-claw:
    image: ag-claw:latest
    ports:
      - "3000:3000"
    environment:
      - OPENROUTER_API_KEY=${OPENROUTER_API_KEY}
      - AGCLAW_PORT=3000
    volumes:
      - ./data:/app/data
      - ./agclaw.json:/app/agclaw.json:ro
    restart: unless-stopped
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:3000/health"]
      interval: 30s
      timeout: 10s
      retries: 3
```

### VPS Deployment

1. Подключитесь по SSH к VPS
2. Установите Docker: `curl -fsSL get.docker.com | bash`
3. Клонируйте репозиторий: `git clone https://github.com/AG064/ag-claw.git`
4. Скопируйте файл `.env` с вашими API-ключами
5. Запустите `npm run docker:build && npm run docker:up`
6. Настройте обратный прокси (nginx/Caddy) для HTTPS
7. Привяжите домен к VPS

### Мониторинг работоспособности

AG-Claw предоставляет эндпоинт проверки здоровья:

```bash
curl http://localhost:3000/health
```

Функция `health-monitoring` запускает периодические проверки всех активных функций и оповещает, если какая-либо становится неработоспособной.

---

## 7. Мониторинг и логирование

### Уровни логирования

Настраиваются через `agclaw.json` или `AGCLAW_LOG_LEVEL`:

```bash
export AGCLAW_LOG_LEVEL=debug  # debug, info, warn, error
```

### Просмотр логов

```bash
# Логи шлюза в реальном времени
agclaw gateway logs

# Фильтрация по уровню
agclaw gateway logs --level error

# Режим отслеживания
agclaw gateway logs --follow

# Экспорт логов
agclaw gateway logs --export ./logs/$(date +%Y%m%d).log
```

### Структурированный формат логов

Логи выводятся как JSON в продакшене (`format: json`) и в красивом формате при разработке (`format: pretty`):

```json
{
  "level": "info",
  "time": "2026-03-23T01:00:00.000Z",
  "feature": "agent",
  "msg": "Processing message",
  "length": 142
}
```

### Эндпоинт метрик

```bash
curl http://localhost:3000/metrics
```

Возвращает метрики в формате Prometheus:
- `agclaw_messages_total` — всего обработано сообщений
- `agclaw_tool_calls_total` — вызовы инструментов по имени
- `agclaw_llm_tokens_total` — использовано токенов (промпт + завершение)
- `agclaw_memory_entries_total` — записей в семантической памяти
- `agclaw_features_active` — количество активных функций

---

## 8. Резервное копирование и восстановление

### Ручное резервное копирование

```bash
# AG-Claw создаёт резервные копии с временными метками
ls ./backups/

# Создать резервную копию вручную
agclaw backup create

# Восстановить из резервной копии
agclaw backup restore backup-2026-03-18T18-58-44
```

### Что входит в резервную копию

- `data/agclaw.db` — основная SQLite-база
- `data/semantic-memory.db` — семантическая память
- `data/knowledge.db` — граф знаний
- `data/sessions.db` — сессии разговоров
- `data/skills-library.db` — установленные навыки
- `data/goals.db` — цели и декомпозиция
- `data/life-domains.db` — домены жизни
- `agclaw.json` — конфигурация

### Автоматические резервные копии

Настройка в `agclaw.json`:

```json
{
  "backup": {
    "enabled": true,
    "intervalHours": 24,
    "retentionDays": 7,
    "path": "./backups"
  }
}
```

### Восстановление после сбоя

1. Остановите шлюз: `agclaw gateway stop`
2. Восстановите файлы из резервной копии: `agclaw backup restore <название-backup>`
3. Перезапустите: `agclaw gateway start`
4. Проверьте: `curl http://localhost:3000/health`

---

## 9. Справочник API

Полная документация [API](./API.md) включает REST-эндпоинты, WebSocket-события, коды ошибок и схему конфигурации.

Краткий справочник:

| Эндпоинт | Метод | Описание |
|---|---|---|
| `/health` | GET | Проверка работоспособности |
| `/metrics` | GET | Метрики Prometheus |
| `/chat` | POST | Отправить сообщение |
| `/memory/search` | GET | Поиск в памяти |
| `/memory/store` | POST | Сохранить запись в памяти |
| `/agents` | GET | Список агентов |
| `/features` | GET | Список всех функций |
| `/features/:name` | POST | Включить/выключить функцию |
| `/config` | GET/PATCH | Просмотр/обновление конфигурации |

---

## 10. Справочник конфигурации

### Полная схема конфигурации

```typescript
interface AGClawConfig {
  name: string;                          // Имя экземпляра
  server: {
    port: number;                        // Порт шлюза (по умолчанию: 18789)
    host: string;                        // Адрес привязки (по умолчанию: 0.0.0.0)
    cors: {
      enabled: boolean;
      origins: string[];
    };
    rateLimit: {
      enabled: boolean;
      windowMs: number;
      maxRequests: number;
    };
  };
  model: {
    provider: 'openrouter' | 'anthropic' | 'openai';
    defaultModel: string;
    fallbackModel?: string;
    maxTokens: number;
    temperature: number;
    retryAttempts: number;
    retryDelayMs: number;
  };
  features: Record<string, {
    enabled: boolean;
    [key: string]: unknown;
  }>;
  channels: {
    telegram?: {
      enabled: boolean;
      token?: string;
      allowedUsers?: number[];
      allowedChats?: number[];
    };
    webchat?: {
      enabled: boolean;
      port: number;
      maxConnections: number;
    };
    // ... другие каналы
  };
  memory: {
    primary: 'sqlite' | 'markdown' | 'supabase';
    path: string;
    selfEvolving: boolean;
    compressionThreshold: number;
    supabaseUrl?: string;
    supabaseKey?: string;
  };
  security: {
    policy: string;
    secrets: 'encrypted' | 'plain';
    auditLog: boolean;
    allowlistMode: 'permissive' | 'strict';
  };
}
```

### Переменные окружения

| Переменная | Описание | Обязательно |
|---|---|---|
| `OPENROUTER_API_KEY` | API-ключ OpenRouter | Да (если не используется другой провайдер) |
| `ANTHROPIC_API_KEY` | API-ключ Anthropic | Нет |
| `OPENAI_API_KEY` | API-ключ OpenAI | Нет |
| `AGCLAW_TELEGRAM_TOKEN` | Токен Telegram-бота | Нет |
| `AGCLAW_FCM_KEY` | Ключ Firebase Cloud Messaging | Нет |
| `AGCLAW_DB_PATH` | Путь к SQLite-базе | Нет |
| `AGCLAW_PORT` | Порт шлюза | Нет |
| `AGCLAW_LOG_LEVEL` | Уровень логирования | Нет |
| `SUPABASE_URL` | URL проекта Supabase | Нет |
| `SUPABASE_KEY` | Anon-ключ Supabase | Нет |

---

*Для продвинутых тем, таких как многопользовательская координация, mesh-workflows и масштабирование, см. [Урок 5: Продвинутые паттерны](./tutorials/05-advanced-patterns_RU.md).*
