# Руководство по быстрому старту

Запустите Argentum за 5 минут. Это руководство проведёт вас от установки до первого работающего агента.

---

## Системные требования

- **Node.js** версии 18 или выше (`node --version` для проверки)
- **npm** версии 9 или выше (поставляется с Node.js)
- **Git** (для клонирования репозитория)
- API-ключ: [OpenRouter](https://openrouter.ai/) (рекомендуется) или [Anthropic](https://anthropic.com/)

---

## Шаг 1 — Установка

```bash
git clone https://github.com/AG064/argentum.git
cd argentum
npm install
```

На этом всё. Argentum использует только быстрые JavaScript-зависимости — без нативной компиляции.

---

## Шаг 2 — Подключение CLI

```bash
npm link
```

Команда `argentum` станет доступна глобально. Проверьте:

```bash
argentum --version
```

---

## Шаг 3 — Инициализация и запуск

```bash
argentum init
```

Эта команда создаёт файл конфигурации `argentum.json` и директорию `data/`. Затем запустите шлюз:

```bash
argentum gateway start --port 3000
```

Проверьте работоспособность:

```bash
curl http://localhost:3000/health
```

Ожидаемый ответ:

```json
{ "status": "ok", "version": "0.0.3", "features": "12/59 active" }
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
argentum gateway restart
```

---

## Первое общение

Отправьте сообщение через Telegram (если настроен) или через веб-чат:

```bash
# Через веб-чат (сначала включите в argentum.json)
curl -X POST http://localhost:3000/chat \
  -H "Content-Type: application/json" \
  -d '{"message": "Привет, что ты умеешь?", "userId": "test-user"}'
```

Попробуйте задать агенту вопрос:

```
Пользователь: Привет
Агент: Привет! Я Argentum, твой AI-ассистент. У меня есть доступ к инструментам,
       которые позволяют мне искать в интернете, читать и записывать файлы,
       выполнять команды и управлять собственной памятью.
       Чем могу помочь?
```

---

## Базовая настройка

Отредактируйте `argentum.json` для настройки вашего окружения:

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

После редактирования перезапустите: `argentum gateway restart`

---

## Подключение Telegram (опционально)

1. Создайте бота через [@BotFather](https://t.me/BotFather) в Telegram
2. Скопируйте токен бота
3. Добавьте его в `argentum.json`:

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

4. Перезапустите: `argentum gateway restart`
5. Откройте Telegram, найдите своего бота и отправьте `/start`

---

## Проверка установки

Запустите встроенную диагностику:

```bash
argentum doctor
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
argentum gateway start --port 4000
```

### "LLM call failed: API key not set"

```bash
# Проверьте, что ключ установлен
echo $OPENROUTER_API_KEY
# Если пусто — установите снова и перезапустите
export OPENROUTER_API_KEY=sk-or-v1-...
argentum gateway restart
```

### "Feature failed to load: dependency not enabled"

Некоторые функции зависят от других. Проверьте зависимости:

```bash
argentum feature <имя-функции>
# Пример: argentum feature knowledge-graph
```

Включите недостающие зависимости, затем перезапустите.

### Ошибки "Database locked"

```bash
# Сначала остановите шлюз
argentum gateway stop
# Удалите файл блокировки
rm -f ./data/*.db-shm ./data/*.wal
# Перезапустите
argentum gateway start
```

### Бот Telegram не отвечает

1. Проверьте токен: `curl -s "https://api.telegram.org/botВАШ_ТОКЕН/getMe"`
2. Посмотрите логи: `argentum gateway logs`
3. Убедитесь, что `allowedUsers` пуст (принимает всех) или содержит ваш ID пользователя

### Поиск в памяти не возвращает результатов

Памяти нужно время для накопления. Попробуйте:

```bash
# Сохраните тестовую запись напрямую
argentum memory store "тестовая память" "Это тестовая запись"
# Затем найдите её
argentum memory search "тест"
```

---

## Следующие шаги

- Прочитайте [Руководство пользователя](./USER_GUIDE_RU.md) для полного обзора всех функций
- Пройдите [Урок 1: Первый агент](./tutorials/01-first-agent_RU.md) для создания вашего первого агента
- Изучите [Руководство по развёртыванию](./tutorials/04-deployment_RU.md) для запуска с Docker
- Просмотрите [все 59 функций](../README.md#features-at-a-glance) и включите нужные

---

*Возникли трудности? Создайте issue на [GitHub](https://github.com/AG064/argentum/issues) или спросите в сообществе.*
