# Руководство 1: Ваш первый агент

В этом руководстве вы запустите Argentum, настроите своего первого агента и проведёте полноценный разговор. К концу вы будете понимать, как сообщения проходят через систему и как настраивать поведение.

**Примерное время:** 15 минут  
**Требования:** Node.js 18+, API-ключ OpenRouter или Anthropic

---

## Что вы создадите

Полностью настроенный экземпляр Argentum, работающий локально, подключённый к языковой модели, с персистентной памятью и возможностью общаться через REST API или веб-интерфейс.

---

## Шаг 1 — Установка Argentum

```bash
git clone https://github.com/AG064/argentum.git
cd argentum
npm install
```

Привяжите CLI, чтобы `argentum` был доступен из любого места:

```bash
npm link
```

Проверьте установку:

```bash
argentum --version
# 0.0.3
```

---

## Шаг 2 — Инициализация проекта

```bash
argentum init
```

Это создаёт:
- `argentum.json` — ваш файл конфигурации
- `data/` — каталог для SQLite-баз данных и данных сессий
- `backups/` — каталог для автоматических резервных копий

Команда init также проверяет наличие необходимых API-ключей и предлагает ввести их, если они отсутствуют.

---

## Шаг 3 — Задайте API-ключ

Создайте файл `.env` в корне проекта:

```bash
cat > .env << 'EOF'
OPENROUTER_API_KEY=sk-or-v1-your-key-here
EOF
```

Или экспортируйте напрямую:

```bash
export OPENROUTER_API_KEY=sk-or-v1-your-key-here
```

Если у вас нет ключа OpenRouter, получите его на [openrouter.ai](https://openrouter.ai). Бесплатный тариф даёт доступ к нескольким моделям с разумными лимитами.

---

## Шаг 4 — Запуск Gateway

```bash
argentum gateway start
```

Вы должны увидеть:

```
✓ Config loaded (argentum.json)
✓ 12 features enabled
✓ Gateway listening on :18789
✓ Agent ready
```

Проверьте работоспособность:

```bash
curl http://localhost:18789/health
```

```json
{
  "success": true,
  "data": {
    "status": "ok",
    "version": "0.0.3",
    "uptime": 3,
    "features": "12/59 active",
    "memory": { "semantic": 0, "knowledge_graph": 0, "sessions": 0 }
  }
}
```

---

## Шаг 5 — Отправьте первое сообщение

Используйте эндпоинт `/chat`:

```bash
curl -X POST http://localhost:18789/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "Привет! Что ты умеешь?", "userId": "tutorial-user"}'
```

Вы получите ответ примерно такого вида:

```json
{
  "success": true,
  "data": {
    "reply": "Привет! Я Argentum, модульный AI-агент...",
    "sessionId": "sess_abc123",
    "model": "anthropic/claude-sonnet-4-20250514",
    "tokens": { "prompt": 87, "completion": 42, "total": 129 },
    "latencyMs": 1342
  }
}
```

Агент ответил через Claude через OpenRouter. Ваше сообщение и ответ теперь сохранены в памяти сессии.

---

## Шаг 6 — Включите веб-чат (опционально)

Модуль веб-чата предоставляет браузерный интерфейс. Включите его в `argentum.json`:

```json
{
  "channels": {
    "webchat": {
      "enabled": true,
      "port": 3000,
      "maxConnections": 50
    }
  }
}
```

Перезапустите gateway:

```bash
argentum gateway restart
```

Откройте `http://localhost:3000` в браузере для общения через веб-интерфейс.

---

## Шаг 7 — Подключите Telegram (опционально)

Чтобы подключить агента к Telegram-боту:

1. Создайте бота через [@BotFather](https://t.me/botfather) в Telegram
2. Скопируйте токен, который даст BotFather
3. Добавьте его в `argentum.json`:

```json
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "7123456789:AAFxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
      "allowedUsers": []
    }
  }
}
```

4. Перезапустите gateway и отправьте `/start` боту в Telegram

Чтобы ограничить доступ конкретным пользователям, добавьте их Telegram ID в `allowedUsers`:

```json
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "7123456789:AAF...",
      "allowedUsers": [123456789]
    }
  }
}
```

---

## Шаг 8 — Настройте агента

Измените имя и личность агента:

```bash
argentum config name "Tutorial Bot"
argentum config agent.systemPrompt "Ты — дружелюбный наставник, который объясняет понятия ясно и даёт примеры. Всегда начинай с краткого обзора перед подробностями."
```

Hot-reload применяет изменения немедленно. Отправьте ещё одно сообщение:

```bash
curl -X POST http://localhost:18789/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "Объясни, что такое дерево бинарного поиска", "userId": "tutorial-user"}'
```

---

## Шаг 9 — Сохраните воспоминание

Argentum запоминает вещи между сессиями. Сохраните факт явно:

```bash
curl -X POST http://localhost:18789/memory/store \
  -H "Content-Type: application/json" \
  -d '{"content": "Меня зовут Алиса, я изучаю Argentum", "tags": ["знакомство", "идентичность"]}'
```

Найдите в памяти:

```bash
curl "http://localhost:18789/memory/search?q=имя%20Argentum"
```

Теперь в новом разговоре агент сможет ссылаться на эту информацию. Начните свежую сессию:

```bash
curl -X POST http://localhost:18789/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "Ты знаешь, как меня зовут?", "userId": "new-session-user"}'
```

Агент извлекает сохранённый факт и отвечает правильно, хотя это другой `userId`.

---

## Шаг 10 — Исследуйте модули

Посмотрите список всех доступных модулей:

```bash
argentum features list
```

Включите новый модуль, например утренний брифинг:

```bash
curl -X POST http://localhost:18789/features/morning-briefing \
  -H "Content-Type: application/json" \
  -d '{"enabled": true}'
```

Или через CLI:

```bash
argentum features enable morning-briefing
```

---

## Как это работает

Вот что происходило за кулисами:

1. **Ваше сообщение** пришло на HTTP-gateway через curl
2. **Проверки безопасности** выполнились (rate limiting, allowlists)
3. **Автозахват** просканировал сообщение на предмет фактов для запоминания
4. **Agentic Tool Loop** отправил сообщение Claude с доступными инструментами
5. **Память** проверена на контекст о вас
6. **Ответ** пришёл и был отправлен вам
7. **Разговор** сохранён в базе данных сессий

```
Вы → HTTP POST → Gateway → Безопасность → Tool Loop → LLM → Память → Вы
```

---

## Что вы узнали

- Как установить и инициализировать Argentum
- Как настроить API-ключ
- Как запустить gateway и проверить работоспособность
- Как отправлять сообщения через REST API
- Как включить веб-чат и канал Telegram
- Как настроить личность агента
- Как сохранять и извлекать воспоминания
- Как включать и выключать модули

---

## Следующие шаги

- **[Руководство 2: Управление памятью](./02-memory-management_RU.md)** — Глубокое погружение в многослойную систему памяти Argentum
- **[Руководство пользователя](../USER_GUIDE_RU.md)** — Полный справочник по всем возможностям Argentum
- **[Руководство разработчика](../DEVELOPER_GUIDE.md)** — Узнайте, как расширять Argentum собственными модулями и каналами
