# Урок 1: Создание первого агента

Узнайте, как создать и настроить своего первого AI-агента с AG-Claw.

## 📚 Что вы узнаете

- Как создать конфигурацию агента
- Настройка базовых возможностей
- Тестирование агента
- Отладка распространённых проблем

## 🚀 Необходимые условия

- AG-Claw установлен ([Руководство по быстрому старту](../QUICK_START_RU.md))
- Базовое понимание JavaScript/TypeScript
- API-ключ (OpenRouter или Anthropic)

## Шаг 1 — Создание конфигурации агента

Создайте файл `agents/my-first-agent.ts`:

```typescript
import type { AgentConfig } from '@ag-claw/core';

export const myAgent: AgentConfig = {
  name: 'Мой первый агент',
  model: 'openrouter/anthropic/claude-sonnet-4',
  temperature: 0.7,
  maxTokens: 4096,
  
  // Возможности
  capabilities: {
    webSearch: true,
    fileAccess: true,
    commandExecution: false, // Отключено для безопасности
  },
  
  // Системный промпт
  systemPrompt: `Вы — полезный ассистент, специализирующийся на {{topic}}.`,
  
  // Настройки памяти
  memory: {
    type: 'episodic', // Хранить разговоры
    retention: 7, // Дней
  },
};

export default myAgent;
```

## Шаг 2 — Регистрация агента

Добавьте в ваш `agclaw.json`:

```json
{
  "agents": {
    "my-first-agent": {
      "enabled": true,
      "config": "./agents/my-first-agent.ts"
    }
  }
}
```

## Шаг 3 — Тестирование агента

Запустите агента:

```bash
agclaw agent start my-first-agent
```

Отправьте тестовое сообщение:

```bash
agclaw agent chat my-first-agent --message "Привет!"
```

Ожидаемый результат:

```
🤖 Агент: Привет! Чем могу помочь?
```

## Шаг 4 — Включение дополнительных возможностей

Добавьте веб-поиск:

```typescript
capabilities: {
  webSearch: {
    provider: 'duckduckgo',
    rateLimit: 10, // запросов в минуту
  },
}
```

Добавьте доступ к файлам:

```typescript
capabilities: {
  fileAccess: {
    allowedPaths: ['/home/user/projects'],
    readOnly: false,
  },
}
```

## Шаг 5 — Отладка проблем

Проверьте логи агента:

```bash
agclaw agent logs my-first-agent --tail 50
```

Распространённые проблемы:

| Проблема | Решение |
|----------|---------|
| Агент не отвечает | Проверьте API-ключ |
| Медленные ответы | Уменьшите `maxTokens` |
| Ошибки памяти | Увеличьте контекстное окно |

## 🎉 Поздравляем!

Вы создали своего первого AG-Claw агента. Следующие шаги:

- [Управление памятью](../02-memory-management_RU.md) — Узнайте, как агенты запоминают
- [Разработка навыков](../03-skill-development_RU.md) — Добавьте собственные возможности
- [Развёртывание](../04-deployment_RU.md) — Разверните в продакшен

## 📞 Нужна помощь?

- 📖 [Руководство пользователя](../USER_GUIDE_RU.md)
- 💬 [Telegram сообщество](https://t.me/agclaw)
- 🐛 [Сообщить об ошибке](../../issues/new?template=bug_report.md)
