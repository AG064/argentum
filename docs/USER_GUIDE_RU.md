# Руководство пользователя

Подробное руководство по эксплуатации Argentum в продакшене и в повседневном использовании. Этот документ охватывает архитектуру, конфигурацию, управление памятью, безопасность, развёртывание, мониторинг и многое другое.

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
9. [Справочник по API](#9-справочник-по-api)
10. [Справочник по конфигурации](#10-справочник-по-конфигурации)

---

## 1. Обзор архитектуры

### Философия проектирования

Argentum построен на трёх принципах:

1. **Модульность важнее монолита** — каждая возможность представляет собой модуль, который можно независимо включить или выключить
2. **Память как полноценный гражданин** — агент не просто обрабатывает сообщения; он запоминает, учится и развивается
3. **Безопасность по умолчанию** — шифрование, аудитлог и политики безопасности встроены, а не прикручены потом

### Диаграмма системы

```
┌──────────────────────────────────────────────────────────────────┐
│                        Инфраструктура                            │
│                                                                  │
│   ┌─────────────────────────────────────────────────────────┐    │
│   │                   Argentum Gateway                        │    │
│   │                       :18789                             │    │
│   ├───────────────┬────────────────────┬────────────────────┤    │
│   │  Каналы        │   Функциональные модули │   Ядро           │    │
│   │               │                    │                     │    │
│   │  • Telegram    │  • sqlite-memory  │  • Plugin Loader   │    │
│   │  • Discord     │  • semantic-search │  • LLM Provider    │    │
│   │  • Webchat     │  • cron-scheduler │  • Config Manager  │    │
│   │  • Slack       │  • morning-brief  │  • Logger          │    │
│   │  • WhatsApp    │  • audit-log      │  • Security Layer   │    │
│   │  • Email       │  • mesh-workflows │                    │    │
│   │  • SMS         │  • ... (53 ещё)   │                     │    │
│   └───────┬───────┴────────┬──────────┴──────────┬──────────┘    │
│           │                 │                      │               │
│           │     ┌───────────▼────────────────────▼─────────┐     │
│           │     │       Agentic Tool Loop                  │     │
│           │     │                                           │     │
│           │     │   Message → LLM → Tool Calls → Memory     │     │
│           │     │                                           │     │
│           │     │   ┌─────────────────────────────────────┐ │     │
│           │     │   │  Semantic Memory  │  Knowledge Graph │ │     │
│           │     │   │  SQLite Store     │  Checkpoint      │ │     │
│           │     │   └─────────────────────────────────────┘ │     │
│           │     └────────────────────────────────────────────┘     │
│           │                                                       │
│   ┌───────▼────────┐                            ┌────────────────▼┐│
│   │   Конечные     │                            │  Внешние API    ││
│   │   пользователи │                            │ OpenRouter,    ││
│   └────────────────┘                            │ Anthropic и др ││
│                                                  └────────────────┘│
└──────────────────────────────────────────────────────────────────┘
```

### Основные компоненты

| Компонент | Файл(ы) | Роль |
|---|---|---|
| **Gateway** | `src/index.ts` | HTTP-сервер, загрузка всех модулей, управление жизненным циклом |
| **CLI** | `src/cli.ts` | Командный интерфейс пользователя |
| **Plugin Loader** | `src/core/plugin-loader.ts` | Обнаружение, загрузка, запуск и проверка работоспособности 59 модулей |
| **Agent** | `src/index.ts` (класс `Agent`) | Agentic Tool Loop — оркестрация LLM + инструменты + память |
| **LLM Provider** | `src/core/llm-provider.ts` | Абстракция над OpenRouter, Anthropic, OpenAI |
| **Memory** | `src/memory/` | Семантический поиск, граф знаний, персистентность в SQLite |
| **Channels** | `src/channels/` | Адаптеры протоколов (Telegram, Discord, Webchat, SMS, Email) |
| **Features** | `src/features/*/index.ts` | 59 отдельных модулей |
| **Security** | `src/security/` | Политический движок, зашифрованные секреты, allowlist |

### Agentic Tool Loop

Когда пользователь отправляет сообщение, Argentum обрабатывает его через следующий цикл:

1. **Сообщение получено** — адаптер канала нормализует входные данные в стандартный формат `Message`
2. **Проверка безопасности** — применение allowlist, rate limiting, фильтрация контента
3. **Автозахват** — модуль автоматически определяет решения, уроки, ошибки в сообщении
4. **Вызов LLM** — сообщение отправляется модели вместе с историей разговора и доступными инструментами
5. **Выполнение инструментов** — если модель вызывает инструмент, он выполняется и результаты возвращаются в контекст
6. **Обновление памяти** — семантическая память сохраняет взаимодействие; граф знаний обновляет связи между сущностями
7. **Ответ** — итоговый текст возвращается пользователю через адаптер канала

Цикл выполняется до 10 итераций на сообщение для обработки сложных многошаговых задач. Настройте это через `agent.maxIterations`.

### Система модулей

Каждый модуль представляет собой самодостаточный компонент в `src/features/<name>/index.ts`. Модули независимы и настраиваются через `argentum.json`. Вы можете включать или выключать любой модуль без изменения кода.

Текущее количество модулей: **59 модулей**, включая аудитлогирование, семантический поиск, планировщик задач, утренние отчёты, mesh-воркфлоры, зашифрованные секреты, отслеживание целей и многое другое.

---

## 2. Создание и управление агентами

### Создание первого агента

После выполнения `argentum init` ваш агент готов к использованию. Конфигурация по умолчанию создаёт универсального агента. Для настройки:

```bash
# Просмотр текущей конфигурации
argentum config

# Установить имя агента
argentum config name "Мой персональный ассистент"

# Установить системный промпт
argentum config agent.systemPrompt "Ты — полезный ассистент..."

# Показать полную конфигурацию
cat argentum.json
```

При первом запуске агент поможет настроить API-ключ. Также можно задать его через переменную окружения:

```bash
export OPENROUTER_API_KEY=sk-or-v1-...
argentum gateway start
```

### Управление несколькими агентами

Argentum поддерживает несколько профилей агентов для разных сценариев использования:

```bash
# Создать агента для программирования
argentum agents create --name "coding-assistant" \
  --system-prompt "Ты — эксперт-программист, специализирующийся на TypeScript и Python..." \
  --model "anthropic/claude-sonnet-4-20250514"
```

Список всех агентов:

```bash
argentum agents list
```

Переключиться на другого агента:

```bash
argentum agents use "coding-assistant"
```

Удалить профиль агента:

```bash
argentum agents delete "coding-assistant"
```

### Конфигурация агента

Ключевые параметры агента в `argentum.json`:

```json
{
  "agent": {
    "name": "Argentum Ассистент",
    "systemPrompt": "Ты — полезный AI-ассистент...",
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
    "retryAttempts": 3,
    "retryDelayMs": 1000
  }
}
```

### Сессии разговора

Argentum хранит историю разговоров для каждого пользователя в `data/sessions.db`. Сессии включают полную историю сообщений, вызовы инструментов и статистику использования LLM.

```bash
# Просмотр недавних сессий
argentum sessions list

# Просмотр конкретной сессии
argentum sessions view <session-id>

# Очистка сессии (сбрасывает разговор)
argentum sessions clear <session-id>

# Экспорт сессии в JSON
argentum sessions export <session-id> > session.json
```

### Смена модели

```bash
# Использовать другую модель по умолчанию
argentum config model.defaultModel "openai/gpt-4o"

# Перезапустить для применения
argentum gateway restart
```

Доступные провайдеры:
- **OpenRouter** — Рекомендуется. Доступ к 100+ моделям включая Claude, GPT-4, Llama, Mistral
- **Anthropic** — Прямой доступ к моделям Claude
- **OpenAI** — GPT-4, GPT-4o, GPT-3.5 Turbo

---

## 3. Система памяти

Argentum имеет многослойную архитектуру памяти. Каждый слой служит разным целям — от краткосрочного контекста до долгосрочных знаний.

### Слои памяти

```
┌────────────────────────────────────────────┐
│           Семантическая память             │  ← Быстрый поиск по ключевым словам
│     (SQLite + модели эмбеддингов)           │     Используется в каждом разговоре
├────────────────────────────────────────────┤
│           Граф знаний                      │  ← Связи между сущностями
│    (узлы + рёбра, обход графа)              │     Используется для рассуждений
├────────────────────────────────────────────┤
│         Markdown память                    │  ← Читаемые заметки
│      (файлы на диске, отслеживаются)        │     Используется для долгосрочных фактов
├────────────────────────────────────────────┤
│          Сессионная память                 │  ← Текущий разговор
│        (в памяти, на сессию)               │     Используется для контекстного окна
└────────────────────────────────────────────┘
```

### Семантическая память

Основной слой памяти. Хранит факты, разговоры и полученные знания в SQLite с полнотекстовым поиском.

```bash
# Явно сохранить что-то
curl -X POST http://localhost:18789/memory/store \
  -H "Content-Type: application/json" \
  -d '{"content": "Алиса изучает TypeScript", "tags": ["человек", "обучение"]}'

# Поиск в памяти
curl "http://localhost:18789/memory/search?q=Алиса%20TypeScript"
```

Ключевые характеристики:
- Автоматическая индексация модулем `self-evolving-memory`
- Записи автоматически категоризируются и тегируются
- Настраиваемая компрессия при превышении порога
- Сохраняется между перезапусками (SQLite)

### Граф знаний

Хранит сущности и их отношения. Позволяет делать выводы на основе связанных фактов.

```bash
# Запрос по сущности
curl "http://localhost:18789/memory/graph?entity=alice"
```

Пример записи графа знаний:
```
Сущность: Алиса (тип: человек)
  - работает_в: Acme Corp (тип: компания)
  - говорит_на: английский, испанский
  - интересы: программирование, AI, космос, фотография
```

Граф автоматически обновляется по мере обработки сообщений агентом.

### Markdown память

Плоские файлы на диске в каталоге `data/memory/`. Отслеживаются файловым наблюдателем — изменения подхватываются сразу без перезапуска.

```bash
# Создать файл памяти
cat > data/memory/facts.md << 'EOF'
# Факты об Алисе

## Предпочтения
- Lark早起 (совенок/жаворонок)
- Предпочитает тёмную тему
- Использует Arch Linux

## Цели
- Найти работу программистом к концу 2026
- Изучить Rust
EOF
```

Markdown-файлы парсятся и интегрируются в контекст агента. Это полезно для:
- Вручную курируемых фактов
- Заметок из других систем
- Информации, которая должна переживать компрессию памяти

### Управление памятью

```bash
# Показать статистику памяти
argentum memory stats

# Очистить старые записи (до определённой даты)
argentum memory purge --before 2026-01-01

# Экспортировать все воспоминания
argentum memory export > memories.json

# Импортировать воспоминания
argentum memory import memories.json
```

Модуль `self-evolving-memory` автоматически:
- Консолидирует похожие воспоминания для экономии места
- Обнаруживает паттерны в хранимой информации
- Применяет затухание к малорелевантным записям

---

## 4. Навыки и как их использовать

Навыки (skills) — это многократно используемые пакеты возможностей, которые расширяют функциональность вашего агента. Они живут в каталоге `skills/` и могут быть установлены, обновлены и удалены независимо от ядра фреймворка.

### Встроенные навыки

В Argentum предустановлено несколько навыков:

| Навык | Что делает |
|---|---|
| `weather` | Текущая погода и прогнозы через wttr.in или Open-Meteo |
| `summarize` | Резюмирование URL, PDF, изображений, аудио, YouTube |
| `gog` | Google Workspace: Gmail, Календарь, Drive, Sheets, Docs |
| `xurl` | Twitter/X API: публикация, ответы, поиск, DM, медиа |
| `himalaya` | CLI email-клиент через IMAP/SMTP |
| `telegram` | Telegram Bot API воркфлоры |

### Список установленных навыков

```bash
argentum skills list
```

### Установка новых навыков

```bash
# Установить из URL или локального пути
argentum skills install https://github.com/example/skill-repo

# Установить из ClawHub маркетплейса
argentum skill install my-skill
```

### Создание собственных навыков

Подробности в [Руководстве разработчика](./DEVELOPER_GUIDE.md#7-how-to-create-a-new-skill). Кратко:

1. Создайте `skills/my-skill/SKILL.md`
2. Напишите `skills/my-skill/src/index.ts` с логикой
3. Добавьте метаданные для интеграции с CLI

### Вызов навыков

Навыки обычно вызываются агентом автоматически, но можно вызвать и напрямую:

```bash
argentum skills run weather --location "Таллинн"
```

---

## 5. Лучшие практики безопасности

Argentum включает несколько слоёв безопасности. Ниже — рекомендуемые практики для продакшен-развёртывания.

### Включите строгий режим

Начните с `security.policy: "strict"` в `argentum.json`:

```json
{
  "security": {
    "policy": "strict",
    "auditLog": true,
    "allowlistMode": "strict"
  }
}
```

В строгом режиме все действия, не разрешённые явно, запрещены. В разрешительном режиме (по умолчанию) всё разрешено, если не заблокировано правилом.

### Allowlist пользователей

Ограничьте доступ конкретным пользователям по их ID платформы:

```json
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "...",
      "allowedUsers": [123456789, 987654321]
    }
  }
}
```

Это предотвращает доступ неавторизованных пользователей, даже если они знают токен Telegram-бота.

### Шифрование секретов

Argentum шифрует секреты при хранении с помощью AES-256-GCM. Задайте ключ:

```bash
export AGCLAW_SESSION_SECRET=$(openssl rand -hex 32)
```

Никогда не коммитьте API-ключи в систему контроля версий. Используйте переменные окружения:

```bash
# Безопасно сохранить секрет
argentum secrets set OPENROUTER_API_KEY "sk-or-v1-..."
```

### Аудитлогирование

Включите лог аудита для отслеживания всех действий:

```json
{
  "security": {
    "auditLog": true
  }
}
```

Просмотр логов аудита:

```bash
argentum audit list --limit 50
argentum audit search --actor alice --since 2026-03-22
```

Лог аудита хранится в `data/argentum.db` в неизменяемой таблице — записи нельзя удалить или изменить.

### Rate Limiting

Защититесь от злоупотреблений:

```json
{
  "server": {
    "rateLimit": {
      "enabled": true,
      "windowMs": 60000,
      "maxRequests": 100
    }
  }
}
```

### Фильтрация контента

Модуль `content-filtering` сканирует сообщения и вывод инструментов на предмет конфиденциальных данных:

```json
{
  "features": {
    "content-filtering": { "enabled": true }
  }
}
```

Автоматически маскирует:
- API-ключи и токены
- Номера кредитных карт
- Номера социального страхования
- Email-адреса и телефонные номера

---

## 6. Варианты развёртывания

### Локальная разработка

```bash
npm install
npm link
argentum init
argentum gateway start
```

Готово. Gateway работает на `http://localhost:18789`.

### Docker (рекомендуется для продакшена)

```bash
cd argentum/docker
cp .env.example .env  # заполните API-ключи
docker compose up -d
```

### Systemd (Linux)

```ini
# /etc/systemd/system/argentum.service
[Unit]
Description=Argentum AI Agent
After=network.target

[Service]
Type=simple
User=ag064
WorkingDirectory=/home/ag064/argentum
ExecStart=/home/ag064/argentum/bin/argentum.js gateway start
Restart=always
RestartSec=5
Environment=OPENROUTER_API_KEY=sk-or-v1-...
Environment=AGCLAW_PORT=18789

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl enable argentum
sudo systemctl start argentum
sudo systemctl status argentum
```

### Обратный прокси (nginx)

```nginx
server {
    listen 443 ssl;
    server_name yourdomain.com;

    ssl_certificate /etc/letsencrypt/live/yourdomain.com/fullchain.pem;
    ssl_certificate_key /etc/letsencrypt/live/yourdomain.com/privkey.pem;

    location / {
        proxy_pass http://127.0.0.1:18789;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection 'upgrade';
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_cache_bypass $http_upgrade;
    }
}
```

---

## 7. Мониторинг и логирование

### Уровни логов

```bash
export AGCLAW_LOG_LEVEL=debug  # debug, info, warn, error
```

В разработке используйте `pretty` формат:

```bash
export AGCLAW_LOG_FORMAT=pretty
```

В продакшене — `json`:

```bash
export AGCLAW_LOG_FORMAT=json
```

### Просмотр логов

```bash
# Логи gateway в реальном времени
argentum gateway logs

# Фильтр по уровню
argentum gateway logs --level error

# Режим отслеживания
argentum gateway logs --follow

# Экспорт
argentum gateway logs --export ./logs/$(date +%Y%m%d).log
```

### Метрики Prometheus

```bash
curl http://localhost:18789/metrics
```

### Проверка здоровья

```bash
curl http://localhost:18789/health
```

---

## 8. Резервное копирование и восстановление

### Ручное резервное копирование

```bash
# Список резервных копий
ls ./backups/

# Создать резервную копию
argentum backup create

# Восстановить из резервной копии
argentum backup restore backup-2026-03-18T18-58-44
```

### Что входит в резервную копию

| Файл | Описание |
|---|---|
| `data/argentum.db` | Основная база данных SQLite |
| `data/semantic-memory.db` | Семантическая память |
| `data/knowledge.db` | Граф знаний |
| `data/sessions.db` | Сессии разговоров |
| `data/skills-library.db` | Установленные навыки |
| `data/goals.db` | Цели и декомпозиция |
| `argentum.json` | Конфигурация |

### Автоматическое резервное копирование

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

### Процедура восстановления

1. Остановить gateway: `argentum gateway stop`
2. Восстановить файлы: `argentum backup restore <backup-name>`
3. Запустить: `argentum gateway start`
4. Проверить: `curl http://localhost:18789/health`

---

## 9. Справочник по API

Полная документация API доступна в [API.md](./API.md).

Краткий справочник:

| Endpoint | Метод | Описание |
|---|---|---|
| `/health` | GET | Проверка здоровья |
| `/metrics` | GET | Метрики Prometheus |
| `/chat` | POST | Отправить сообщение агенту |
| `/chat/stream` | POST | Потоковый ответ (SSE) |
| `/memory/search` | GET | Поиск в семантической памяти |
| `/memory/store` | POST | Сохранить запись в память |
| `/memory/graph` | GET | Запрос к графу знаний |
| `/agents` | GET/POST | Список или создание агентов |
| `/features` | GET | Список всех модулей |
| `/config` | GET/PATCH | Просмотр или изменение конфигурации |

---

## 10. Справочник по конфигурации

### Основные параметры

```json
{
  "name": "Argentum",
  "version": "0.0.5",
  "server": {
    "port": 18789,
    "host": "0.0.0.0",
    "cors": { "enabled": true, "origins": [] },
    "rateLimit": { "enabled": true, "windowMs": 60000, "maxRequests": 100 }
  },
  "agent": {
    "name": "Argentum Ассистент",
    "maxIterations": 10,
    "temperature": 0.7
  },
  "model": {
    "provider": "openrouter",
    "defaultModel": "anthropic/claude-sonnet-4-20250514",
    "fallbackModel": "openai/gpt-4o",
    "maxTokens": 8192,
    "temperature": 0.7,
    "retryAttempts": 3
  },
  "features": {
    "audit-log": { "enabled": true },
    "sqlite-memory": { "enabled": true },
    "semantic-search": { "enabled": true },
    "morning-briefing": { "enabled": true }
  },
  "memory": {
    "primary": "sqlite",
    "path": "./data",
    "selfEvolving": true,
    "compressionThreshold": 50000
  },
  "security": {
    "policy": "permissive",
    "secrets": "encrypted",
    "auditLog": true,
    "allowlistMode": "permissive"
  },
  "backup": {
    "enabled": true,
    "intervalHours": 24,
    "retentionDays": 7,
    "path": "./backups"
  }
}
```

### Переменные окружения

| Переменная | Описание | Обязательно |
|---|---|---|
| `OPENROUTER_API_KEY` | OpenRouter API ключ | Да |
| `ANTHROPIC_API_KEY` | Anthropic API ключ | Нет |
| `OPENAI_API_KEY` | OpenAI API ключ | Нет |
| `AGCLAW_PORT` | Порт gateway (по умолчанию 18789) | Нет |
| `AGCLAW_HOST` | Адрес привязки (по умолчанию 0.0.0.0) | Нет |
| `AGCLAW_DB_PATH` | Путь к SQLite (по умолчанию ./data/argentum.db) | Нет |
| `AGCLAW_LOG_LEVEL` | Уровень логирования | Нет |
| `AGCLAW_LOG_FORMAT` | Формат логов (pretty/json) | Нет |
| `AGCLAW_TELEGRAM_TOKEN` | Токен Telegram-бота | Нет |
| `AGCLAW_SESSION_SECRET` | Ключ для шифрования сессий | Нет |
| `SUPABASE_URL` | URL проекта Supabase | Нет |
| `SUPABASE_KEY` | Anon ключ Supabase | Нет |

---

*Пошаговые руководства доступны в каталоге [tutorials](./tutorials/).*
