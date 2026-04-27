# Tutorial 4: Deployment

*Estimated time: 25 minutes*

This tutorial covers deploying Argentum in production. We'll look at Docker deployment, VPS setup, reverse proxy configuration, and production hardening.

---

## Deployment Options

| Method | Best For | Complexity |
|---|---|---|
| **Local** | Development, testing | Minimal |
| **Docker** | Single-server production | Low |
| **VPS** | Internet-accessible deployment | Medium |
| **Kubernetes** | Multi-node, scaled deployments | High |

This tutorial focuses on Docker and VPS deployment.

---

## Docker Deployment (Recommended)

### Prerequisites

- Docker installed (`docker --version`)
- Docker Compose installed (`docker compose version`)

### Build the Image

```bash
cd argentum
npm run docker:build
```

This builds the Argentum image with all dependencies baked in.

### Configure Environment

Create a `.env` file in the project root:

```bash
# Required
OPENROUTER_API_KEY=sk-or-v1-...

# Optional
AGCLAW_PORT=3000
AGCLAW_LOG_LEVEL=info
AGCLAW_DB_PATH=/app/data
```

### Start the Container

```bash
npm run docker:up
```

The gateway starts in the background. Check status:

```bash
docker compose -f docker/docker-compose.yml ps
```

### View Logs

```bash
docker compose -f docker/docker-compose.yml logs -f
```

Or for a specific service:

```bash
docker compose -f docker/docker-compose.yml logs -f argentum
```

### Stop the Container

```bash
npm run docker:down
```

---

## Customizing docker-compose.yml

The default `docker/docker-compose.yml` is a starting point. Customize it:

```yaml
version: '3.8'

services:
  argentum:
    image: argentum:latest
    container_name: argentum
    restart: unless-stopped
    ports:
      - "3000:3000"       # Gateway
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

## VPS Deployment

### Step 1 — Choose a VPS Provider

Recommended providers:
- **Hetzner** — Good price/performance, EU data centers
- **DigitalOcean** — Easy setup, good docs
- **Vultr** — Global presence, fast SSDs
- **AWS EC2** — Maximum flexibility, complex pricing

Minimum recommended specs:
- 2 vCPU
- 4 GB RAM
- 40 GB SSD
- Ubuntu 22.04 LTS

### Step 2 — Set Up the VPS

```bash
# SSH into your VPS
ssh root@your-vps-ip

# Update system
apt update && apt upgrade -y

# Install Docker
curl -fsSL get.docker.com | bash

# Install Docker Compose
apt install docker-compose -y

# Create a non-root user (recommended)
adduser argentum
usermod -aG docker argentum
su - argentum
```

### Step 3 — Clone and Configure

```bash
# As the argentum user
git clone https://github.com/AG064/argentum.git
cd argentum

# Create .env
cat > .env << 'EOF'
OPENROUTER_API_KEY=sk-or-v1-...
AGCLAW_PORT=3000
AGCLAW_LOG_LEVEL=info
AGCLAW_API_TOKEN=your-secure-random-token
EOF

# Build and start
npm run docker:build
npm run docker:up
```

### Step 4 — Verify

```bash
curl http://localhost:3000/health
```

### Step 5 — Set Up a Systemd Service (Alternative to Docker)

If you prefer running without Docker:

```bash
# Create the service file
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

# Enable and start
sudo systemctl enable argentum
sudo systemctl start argentum

# Check status
sudo systemctl status argentum
```

---

## Reverse Proxy with HTTPS

For public-facing deployments, use a reverse proxy to terminate HTTPS.

### Option A — Caddy (Recommended)

Caddy automatically handles HTTPS certificates:

```bash
# Install Caddy
apt install -y caddy
```

Configure `/etc/caddy/Caddyfile`:

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

### Option B — Nginx

```bash
apt install -y nginx certbot python3-certbot-nginx
```

Configure `/etc/nginx/sites-available/argentum`:

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

Enable the site and get a certificate:

```bash
ln -s /etc/nginx/sites-available/argentum /etc/nginx/sites-enabled/
nginx -t
certbot --nginx -d argentum.example.com
systemctl reload nginx
```

---

## Production Hardening

### 1. Secure the API Token

```bash
# Generate a strong random token
openssl rand -base64 32
```

Add it to your `.env`:

```bash
AGCLAW_API_TOKEN=your-generated-token-here
```

### 2. Configure Allowlists

Restrict access to specific users:

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

### 3. Enable Rate Limiting

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

### 4. Set Up Automated Backups

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

For VPS, also configure off-server backups:

```bash
# Example: Rsync backups to another machine
0 2 * * * rsync -avz /home/argentum/argentum/data/ backup@backup-server:/backups/argentum/
```

### 5. Firewall

```bash
# UFW on Ubuntu
ufw default deny incoming
ufw default allow outgoing
ufw allow ssh
ufw allow 443/tcp  # HTTPS
ufw allow 80/tcp   # HTTP (for certbot)
ufw enable
```

### 6. Log Rotation

Configure log rotation for the gateway logs:

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

## Monitoring in Production

### Health Check Script

Create a monitoring script:

```bash
#!/bin/bash
# /usr/local/bin/check-argentum.sh

response=$(curl -sf http://localhost:3000/health)
if [ $? -ne 0 ]; then
    echo "Argentum is down!"
    # Send alert (configure your alerting method)
    exit 1
fi

echo "Argentum is healthy: $response"
```

Add to crontab:

```bash
*/5 * * * * /usr/local/bin/check-argentum.sh >> /var/log/argentum-health.log 2>&1
```

### Prometheus Metrics

Enable the metrics endpoint and scrape with Prometheus:

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

## Updating Argentum

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

## Troubleshooting

| Problem | Solution |
|---|---|
| Container won't start | Check logs: `docker compose logs argentum` |
| Health check failing | Verify port is correct; check `curl http://localhost:3000/health` directly |
| Can't connect from internet | Check firewall: `ufw status`; check reverse proxy logs |
| HTTPS not working | Verify domain DNS points to VPS; check Caddy/Nginx logs |
| Memory usage too high | Reduce `maxConnections` in webchat config; enable memory compression |
| Slow responses | Check if `semantic-search` indexing is running; reduce message history |

---

## Next Steps

- **[Tutorial 5: Advanced Patterns](./05-advanced-patterns.md)** — Multi-agent coordination, mesh workflows, scaling
- **[API Reference](../API.md)** — Full REST and WebSocket API documentation

---

*Questions? Open an issue on [GitHub](https://github.com/AG064/argentum/issues).*
