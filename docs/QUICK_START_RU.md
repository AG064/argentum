# Руководство по быстрому старту

Запустите AG-Claw за 5 минут. Это руководство проведёт вас от установки до первого работающего агента.

---

## Системные требования

- **Node.js** версии 18 или выше (`node --version` для проверки)
- **npm** версии 9 или выше (поставляется с Node.js)
- **Git** (для клонирования репозитория)
- API-ключ: [OpenRouter](https://openrouter.ai/) (рекомендуется) или [Anthropic](https://anthropic.com/)

---

## Шаг 1 — Установка

```bash
git clone https://github.com/AG064/ag-claw.git
cd ag-claw
npm install
```

На этом всё. AG-Claw использует только быстрые JavaScript-зависимости — без нативной компиляции.

---

## Шаг 2 — Подключение CLI

```bash
npm link
```

Команда `agclaw` станет доступна глобально. Проверьте:

```bash
agclaw --version
```

---

## Шаг 3 — Инициализация и запуск

```bash
agclaw init
```

Эта команда создаёт файл конфигурации `agclaw.json` и директорию `data/`. Затем запустите шлюз:

```bash
agclaw gateway start --port 3000
```

Проверьте работоспособность:

```bash
curl http://localhost:3000/health
```

Ожидаемый ответ:

```json
{ "status": "ok", "version": "0.0.2", "features": "12/59 active" }
```

---

## Установите свой API-ключ

Агенту нужен API-ключ LLM для работы. Установите его перед началом общения:

```bash
# Вариант А: OpenRouter (рекомендуется — доступ ко множеству моделей)
export OPENROUTER_API_KEY=sk-or-v1-...

# Вариант Б: Anthropic напрямую
export ANTHROPIC_API_KEY=sk-ant-...

# Вариант В: OpenAI (для Whisper STT, генерации изображений)
export OPENAI_API_KEY=sk-...
```

Перезапустите, чтобы применить ключ:

```bash
agclaw gateway restart
```

---

## Первое общение

Отправьте сообщение через Telegram (если настроен) или через веб-чат:

```bash
# Через веб-чат (сначала включите в agclaw.json)
curl -X POST http://localhost:3000/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "Привет, что ты умеешь?", "userId": "test-user"}'
```

Попробуйте задать агенту вопрос:

```
Пользователь: Привет
Агент: Привет! Я AG-Claw, твой AI-ассистент. У меня есть доступ к инструментам,
       которые позволяют мне искать в интернете, читать и записывать файлы,
       выполнять команды и управлять собственной памятью.
       Чем могу помочь?
```

---

## Базовая настройка

Отредактируйте `agclaw.json` для настройки вашего окружения:

```json
{
  "name": "Мой Агент",
  "server": {
    "port": 3000
  },
  "model": {
    "provider": "openrouter",
    "defaultModel": "anthropic/claude-sonnet-4-20250514"
  },
  "features": {
    "sqlite-memory": { "enabled": true },
    "semantic-search": { "enabled": true },
    "cron-scheduler": { "enabled": true },
    "morning-briefing": { "enabled": true },
    "telegram": { "enabled": false },
    "webchat": { "enabled": true }
  },
  "channels": {
    "telegram": {
      "enabled": false,
      "allowedUsers": []
    }
  }
}
```

Основные разделы конфигурации:

| Раздел | Назначение |
|---|---|
| `server.port` | HTTP-порт шлюза |
| `model.provider` | `openrouter`, `anthropic` или `openai` |
| `model.defaultModel` | Какую модель использовать |
| `features` | Включение/выключение 59 функций |
| `channels` | Настройка каналов связи |
| `security` | Ограничение скорости, списки доступа, аудит |

После редактирования перезапустите: `agclaw gateway restart`

---

## Подключение Telegram (опционально)

1. Создайте бота через [@BotFather](https://t.me/BotFather) в Telegram
2. Скопируйте токен бота
3. Добавьте его в `agclaw.json`:

```json
{
  "channels": {
    "telegram": {
      "enabled": true,
      "token": "ВАШ_ТОКЕН_БОТА",
      "allowedUsers": []
    }
  }
}
```

4. Перезапустите: `agclaw gateway restart`
5. Откройте Telegram, найдите своего бота и отправьте `/start`

---

## Проверка установки

Запустите встроенную диагностику:

```bash
agclaw doctor
```

Ожидаемый результат:

```
✓ Версия Node.js (v20.x.x)
✓ npm установлен
✓ Файл конфигурации найден
✓ Директория data существует
✓ База данных инициализирована
✓ Порт шлюза свободен
⚠  OPENROUTER_API_KEY не установлен (возможности ограничены)
✓ Токен Telegram настроен (канал готов)
```

---

## Решение распространённых проблем

### "Gateway failed to start: port already in use"

```bash
# Найдите и остановите процесс, занимающий порт
lsof -ti:3000 | xargs kill -9
# Или используйте другой порт
agclaw gateway start --port 4000
```

### "LLM call failed: API key not set"

```bash
# Проверьте, что ключ установлен
echo $OPENROUTER_API_KEY
# Если пусто — установите снова и перезапустите
export OPENROUTER_API_KEY=sk-or-v1-...
agclaw gateway restart
```

### "Feature failed to load: dependency not enabled"

Некоторые функции зависят от других. Проверьте зависимости:

```bash
agclaw feature <имя-функции>
# Пример: agclaw feature knowledge-graph
```

Включите недостающие зависимости, затем перезапустите.

### Ошибки "Database locked"

```bash
# Сначала остановите шлюз
agclaw gateway stop
# Удалите файл блокировки
rm -f ./data/*.db-shm ./data/*.wal
# Перезапустите
agclaw gateway start
```

### Бот Telegram не отвечает

1. Проверьте токен: `curl -s "https://api.telegram.org/botВАШ_ТОКЕН/getMe"`
2. Посмотрите логи: `agclaw gateway logs`
3. Убедитесь, что `allowedUsers` пуст (принимает всех) или содержит ваш ID пользователя

### Поиск в памяти не возвращает результатов

Памяти нужно время для накопления. Попробуйте:

```bash
# Сохраните тестовую запись напрямую
agclaw memory store "тестовая память" "Это тестовая запись"
# Затем найдите её
agclaw memory search "тест"
```

---

## Следующие шаги

- Прочитайте [Руководство пользователя](./USER_GUIDE_RU.md) для полного обзора всех функций
- Пройдите [Урок 1: Первый агент](./tutorials/01-first-agent_RU.md) для создания вашего первого агента
- Изучите [Руководство по развёртыванию](./tutorials/04-deployment_RU.md) для запуска с Docker
- Просмотрите [все 59 функций](../README.md#features-at-a-glance) и включите нужные

---

*Возникли трудности? Создайте issue на [GitHub](https://github.com/AG064/ag-claw/issues) или спросите в сообществе.*
