# Урок 4: Развёртывание

*Примерное время: 25 минут*

Этот урок охватывает развёртывание AG-Claw в продакшене. Мы рассмотрим развёртывание с Docker, настройку VPS, конфигурацию обратного прокси и production-hardening.

---

## Варианты развёртывания

| Метод | Подходит для | Сложность |
|---|---|---|
| **Локальный** | Разработка, тестирование | Минимальная |
| **Docker** | Продакшен на одном сервере | Низкая |
| **VPS** | Развёртывание с доступом из интернета | Средняя |
| **Kubernetes** | Мультиузловые, масштабируемые развёртывания | Высокая |

Этот урок фокусируется на Docker и VPS развёртывании.

---

## Docker-развёртывание (рекомендуется)

### Требования

- Docker установлен (`docker --version`)
- Docker Compose установлен (`docker compose version`)

### Сборка образа

```bash
cd ag-claw
npm run docker:build
```

Это собирает образ AG-Claw со всеми зависимостями внутри.

### Настройка окружения

Создайте файл `.env` в корне проекта:

```bash
# Обязательно
OPENROUTER_API_KEY=sk-or-v1-...

# Опционально
AGCLAW_PORT=3000
AGCLAW_LOG_LEVEL=info
AGCLAW_DB_PATH=/app/data
```

### Запуск контейнера

```bash
npm run docker:up
```

Шлюз запускается в фоновом режиме. Проверьте статус:

```bash
docker compose -f docker/docker-compose.yml ps
```

### Просмотр логов

```bash
docker compose -f docker/docker-compose.yml logs -f
```

Или для конкретного сервиса:

```bash
docker compose -f docker/docker-compose.yml logs -f ag-claw
```

### Остановка контейнера

```bash
npm run docker:down
```

---

## Настройка docker-compose.yml

Файл `docker/docker-compose.yml` по умолчанию — это отправная точка. Настройте его:

```yaml
version: '3.8'

services:
  ag-claw:
    image: ag-claw:latest
    container_name: ag-claw
    restart: unless-stopped
    ports:
      - "3000:3000"       # Шлюз
      - "3001:3001"       # Webchat
    environment:
      - OPENROUTER_API_KEY=${OPENROUTER_API_KEY}
      - AGCLAW_PORT=3000
      - AGCLAW_LOG_LEVEL=info
      - AGCLAW_API_TOKEN=${AGCLAW_API_TOKEN}
    volumes:
      - ./data:/app/data
      - ./agclaw.json:/app/agclaw.json:ro
      - ./memory:/app/memory:ro
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:3000/health"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 10s
    networks:
      - ag-claw-net

networks:
  ag-claw-net:
    driver: bridge
```

---

## VPS-развёртывание

### Шаг 1 — Выберите VPS-провайдера

Рекомендуемые провайдеры:
- **Hetzner** — Хорошее соотношение цена/производительность, EU дата-центры
- **DigitalOcean** — Простая настройка, хорошая документация
- **Vultr** — Глобальное присутствие, быстрые SSD
- **AWS EC2** — Максимальная гибкость, сложное ценообразование

Минимальные рекомендуемые характеристики:
- 2 vCPU
- 4 GB RAM
- 40 GB SSD
- Ubuntu 22.04 LTS

### Шаг 2 — Настройте VPS

```bash
# SSH на VPS
ssh root@your-vps-ip

# Обновите систему
apt update && apt upgrade -y

# Установите Docker
curl -fsSL get.docker.com | bash

# Установите Docker Compose
apt install docker-compose -y

# Создайте не-root пользователя (рекомендуется)
adduser agclaw
usermod -aG docker agclaw
su - agclaw
```

### Шаг 3 — Клонируйте и настройте

```bash
# От имени agclaw пользователя
git clone https://github.com/AG064/ag-claw.git
cd ag-claw

# Создайте .env
cat > .env << 'EOF'
OPENROUTER_API_KEY=sk-or-v1-...
AGCLAW_PORT=3000
AGCLAW_LOG_LEVEL=info
AGCLAW_API_TOKEN=your-secure-random-token
EOF

# Соберите и запустите
npm run docker:build
npm run docker:up
```

### Шаг 4 — Проверьте

```bash
curl http://localhost:3000/health
```

### Шаг 5 — Настройте systemd-сервис (альтернатива Docker)

Если предпочитаете запускать без Docker:

```bash
# Создайте файл сервиса
sudo cat > /etc/systemd/system/agclaw.service << 'EOF'
[Unit]
Description=AG-Claw AI Agent
After=network.target

[Service]
Type=simple
User=agclaw
WorkingDirectory=/home/agclaw/ag-claw
ExecStart=/usr/bin/node /home/agclaw/ag-claw/dist/cli.js gateway start
Restart=on-failure
RestartSec=10
Environment=NODE_ENV=production
EnvironmentFile=/home/agclaw/ag-claw/.env

[Install]
WantedBy=multi-user.target
EOF

# Включите и запустите
sudo systemctl enable agclaw
sudo systemctl start agclaw

# Проверьте статус
sudo systemctl status agclaw
```

---

## Обратный прокси с HTTPS

Для публичных развёртываний используйте обратный прокси для терминации HTTPS.

### Вариант A — Caddy (рекомендуется)

Caddy автоматически обрабатывает HTTPS-сертификаты:

```bash
# Установите Caddy
apt install -y caddy
```

Настройте `/etc/caddy/Caddyfile`:

```
agclaw.example.com {
    reverse_proxy localhost:3000
    log {
        output file /var/log/caddy/agclaw.log
    }
}
```

```bash
systemctl reload caddy
```

### Вариант Б — Nginx

```bash
apt install -y nginx certbot python3-certbot-nginx
```

Настройте `/etc/nginx/sites-available/agclaw`:

```nginx
server {
    listen 80;
    server_name agclaw.example.com;

    location / {
        proxy_pass http://127.0.0.1:3000;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection 'upgrade';
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_cache_bypass $http_upgrade;
    }
}
```

Включите сайт и получите сертификат:

```bash
ln -s /etc/nginx/sites-available/agclaw /etc/nginx/sites-enabled/
nginx -t
certbot --nginx -d agclaw.example.com
systemctl reload nginx
```

---

## Production Hardening

### 1. Защитите API-токен

```bash
# Сгенерируйте сильный случайный токен
openssl rand -base64 32
```

Добавьте его в ваш `.env`:

```bash
AGCLAW_API_TOKEN=your-generated-token-here
```

### 2. Настройте списки доступа

Ограничьте доступ для конкретных пользователей:

```json
{
  "channels": {
    "telegram": {
      "allowedUsers": [123456789],
      "allowedChats": [-1001234567890]
    }
  }
}
```

### 3. Включите ограничение скорости

```json
{
  "security": {
    "rateLimit": {
      "enabled": true,
      "windowMs": 60000,
      "maxRequests": 30
    }
  }
}
```

### 4. Настройте автоматические резервные копии

```json
{
  "backup": {
    "enabled": true,
    "intervalHours": 24,
    "retentionDays": 7,
    "path": "/app/backups"
  }
}
```

Для VPS также настройте резервные копии на другом сервере:

```bash
# Пример: Rsync резервные копии на другую машину
0 2 * * * rsync -avz /home/agclaw/ag-claw/data/ backup@backup-server:/backups/ag-claw/
```

### 5. Фаервол

```bash
# UFW на Ubuntu
ufw default deny incoming
ufw default allow outgoing
ufw allow ssh
ufw allow 443/tcp  # HTTPS
ufw allow 80/tcp   # HTTP (для certbot)
ufw enable
```

### 6. Ротация логов

Настройте ротацию логов для логов шлюза:

```bash
# /etc/logrotate.d/agclaw
/home/agclaw/ag-claw/logs/*.log {
    daily
    rotate 14
    compress
    delaycompress
    missingok
    notifempty
    create 0644 agclaw agclaw
}
```

---

## Мониторинг в продакшене

### Скрипт проверки здоровья

Создайте скрипт мониторинга:

```bash
#!/bin/bash
# /usr/local/bin/check-agclaw.sh

response=$(curl -sf http://localhost:3000/health)
if [ $? -ne 0 ]; then
    echo "AG-Claw is down!"
    # Send alert (настройте свой метод оповещения)
    exit 1
fi

echo "AG-Claw is healthy: $response"
```

Добавьте в crontab:

```bash
*/5 * * * * /usr/local/bin/check-agclaw.sh >> /var/log/agclaw-health.log 2>&1
```

### Prometheus Metrics

Включите эндпоинт метрик и собирайте их через Prometheus:

```json
{
  "features": {
    "api-gateway": {
      "enabled": true
    }
  }
}
```

Prometheus scrape config:

```yaml
scrape_configs:
  - job_name: 'ag-claw'
    static_configs:
      - targets: ['localhost:3000']
    metrics_path: '/metrics'
```

---

## Обновление AG-Claw

### Docker Update

```bash
cd ag-claw
git pull
npm run docker:build
docker compose -f docker/docker-compose.yml down
npm run docker:up
```

### VPS Update (systemd)

```bash
cd ag-claw
git pull
npm install
npm run build
sudo systemctl restart agclaw
```

---

## Решение проблем

| Проблема | Решение |
|---|---|
| Контейнер не запускается | Проверьте логи: `docker compose logs ag-claw` |
| Health check failing | Убедитесь, что порт правильный; проверьте `curl http://localhost:3000/health` напрямую |
| Не подключиться из интернета | Проверьте фаервол: `ufw status`; проверьте логи обратного прокси |
| HTTPS не работает | Проверьте, что DNS домена указывает на VPS; проверьте логи Caddy/Nginx |
| Слишком высокая нагрузка на память | Уменьшите `maxConnections` в конфигурации webchat; включите сжатие памяти |
| Медленные ответы | Проверьте, не работает ли индексация `semantic-search`; уменьшите историю сообщений |

---

## Следующие шаги

- **[Урок 5: Продвинутые паттерны](./05-advanced-patterns_RU.md)** — Мультиагентная координация, mesh-workflows, масштабирование
- **[Справочник API](../API.md)** — Полная документация REST и WebSocket API

---

*Вопросы? Создайте issue на [GitHub](https://github.com/AG064/ag-claw/issues).*
