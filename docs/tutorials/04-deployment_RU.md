# Урок 4: Развёртывание

*Примерное время: 25 минут*

Этот урок охватывает развёртывание Argentum в продакшене. Мы рассмотрим развёртывание с Docker, настройку VPS, конфигурацию обратного прокси и production-hardening.

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
cd argentum
npm run docker:build
```

Это собирает образ Argentum со всеми зависимостями внутри.

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
docker compose -f docker/docker-compose.yml logs -f argentum
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
  argentum:
    image: argentum:latest
    container_name: argentum
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
      - ./argentum.json:/app/argentum.json:ro
      - ./memory:/app/memory:ro
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:3000/health"]
      interval: 30s
      timeout: 10s
      retries: 3
      start_period: 10s
    networks:
      - argentum-net

networks:
  argentum-net:
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
adduser argentum
usermod -aG docker argentum
su - argentum
```

### Шаг 3 — Клонируйте и настройте

```bash
# От имени argentum пользователя
git clone https://github.com/AG064/argentum.git
cd argentum

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
sudo cat > /etc/systemd/system/argentum.service << 'EOF'
[Unit]
Description=Argentum AI Agent
After=network.target

[Service]
Type=simple
User=argentum
WorkingDirectory=/home/argentum/argentum
ExecStart=/usr/bin/node /home/argentum/argentum/dist/cli.js gateway start
Restart=on-failure
RestartSec=10
Environment=NODE_ENV=production
EnvironmentFile=/home/argentum/argentum/.env

[Install]
WantedBy=multi-user.target
EOF

# Включите и запустите
sudo systemctl enable argentum
sudo systemctl start argentum

# Проверьте статус
sudo systemctl status argentum
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
argentum.example.com {
    reverse_proxy localhost:3000
    log {
        output file /var/log/caddy/argentum.log
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

Настройте `/etc/nginx/sites-available/argentum`:

```nginx
server {
    listen 80;
    server_name argentum.example.com;

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
ln -s /etc/nginx/sites-available/argentum /etc/nginx/sites-enabled/
nginx -t
certbot --nginx -d argentum.example.com
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
0 2 * * * rsync -avz /home/argentum/argentum/data/ backup@backup-server:/backups/argentum/
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
# /etc/logrotate.d/argentum
/home/argentum/argentum/logs/*.log {
    daily
    rotate 14
    compress
    delaycompress
    missingok
    notifempty
    create 0644 argentum argentum
}
```

---

## Мониторинг в продакшене

### Скрипт проверки здоровья

Создайте скрипт мониторинга:

```bash
#!/bin/bash
# /usr/local/bin/check-argentum.sh

response=$(curl -sf http://localhost:3000/health)
if [ $? -ne 0 ]; then
    echo "Argentum is down!"
    # Send alert (настройте свой метод оповещения)
    exit 1
fi

echo "Argentum is healthy: $response"
```

Добавьте в crontab:

```bash
*/5 * * * * /usr/local/bin/check-argentum.sh >> /var/log/argentum-health.log 2>&1
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
  - job_name: 'argentum'
    static_configs:
      - targets: ['localhost:3000']
    metrics_path: '/metrics'
```

---

## Обновление Argentum

### Docker Update

```bash
cd argentum
git pull
npm run docker:build
docker compose -f docker/docker-compose.yml down
npm run docker:up
```

### VPS Update (systemd)

```bash
cd argentum
git pull
npm install
npm run build
sudo systemctl restart argentum
```

---

## Решение проблем

| Проблема | Решение |
|---|---|
| Контейнер не запускается | Проверьте логи: `docker compose logs argentum` |
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

*Вопросы? Создайте issue на [GitHub](https://github.com/AG064/argentum/issues).*
