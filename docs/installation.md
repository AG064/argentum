# Argentum Installation Guide

```
╔══════════════════════════════════════════════════════════════════╗
║   ██████╗ ██████╗  ██████╗██╗  ██╗██╗   ██╗███████╗          ║
║   ██╔══██╗██╔══██╗██╔════╝██║  ██║██║   ██║██╔════╝          ║
║   ██████╔╝██████╔╝██║     ███████║██║   ██║███████╗          ║
║   ██╔══██╗██╔══██╗██║     ██╔══██║██║   ██║╚════██║          ║
║   ██║  ██║██║  ██║╚██████╗██║  ██║╚██████╔╝███████║          ║
║   ╚═╝  ╚═╝╚═╝  ╚═╝ ╚═════╝╚═╝  ╚═╝ ╚═════╝ ╚══════╝          ║
║                    Modular AI Agent Framework                       ║
╚══════════════════════════════════════════════════════════════════╝
```

## Prerequisites

| Requirement | Minimum Version | Check |
|---|---|---|
| Node.js | >= 20.0 | `node -v` |
| npm | >= 9.0 | `npm -v` |
| Git | any | `git --version` |
| Docker | >= 24.0 (optional) | `docker --version` |
| Docker Compose | >= 2.0 (optional) | `docker compose version` |

### Installing Node.js

**Ubuntu / Debian:**
```bash
curl -fsSL https://deb.nodesource.com/setup_20.x | sudo -E bash -
sudo apt-get install -y nodejs
```

**macOS:**
```bash
brew install node
```

**Windows (PowerShell):**
```powershell
winget install OpenJS.NodeJS.LTS
```

**Verify:**
```bash
node -v   # Should print v20.x.x or higher
npm -v    # Should print 9.x.x or higher
```

---

## Quick Start

The fastest way to get running:

```bash
# Download and run the installer
curl -fsSL https://raw.githubusercontent.com/AG064/argentum/main/install.sh | bash

# Configure your API keys
nano .env

# Start Argentum
npm start
```

Argentum will be available at `http://localhost:18789`.

---

## Windows Installer

Download the latest Windows assets from the GitHub release page:

- `argentum-v0.0.3-win-x64.msi` for a normal Windows install
- `argentum-v0.0.3-win-x64.exe` for a portable executable

The MSI installs `argentum.exe` into `Program Files\Argentum` and adds it to the system `PATH`, so a new terminal can run:

```powershell
argentum --version
argentum onboard
```

See [RELEASE_PACKAGING.md](RELEASE_PACKAGING.md) for release artifact details.

---

## Docker Installation

### Using Docker Compose (Recommended)

```bash
# Clone the repository
git clone https://github.com/AG064/argentum.git
cd argentum

# Create your environment file
cp .env.example .env
nano .env   # Fill in your API keys

# Start with Docker Compose
docker compose -f docker/docker-compose.yml up -d

# View logs
docker compose -f docker/docker-compose.yml logs -f

# Stop
docker compose -f docker/docker-compose.yml down
```

### With Redis (Rate Limiting)

```bash
docker compose -f docker/docker-compose.yml --profile with-redis up -d
```

### Building the Image Manually

```bash
docker build -f docker/Dockerfile -t argentum .
docker run -d \
  --name argentum \
  -p 18789:18789 \
  -p 3001:3001 \
  -v argentum-data:/app/data \
  -v argentum-memory:/app/memory \
  --env-file .env \
  argentum
```

---

## Manual Installation

### 1. Clone the Repository

```bash
git clone https://github.com/AG064/argentum.git
cd argentum
```

### 2. Install Dependencies

```bash
npm install
```

### 3. Configure

```bash
# Copy environment template
cp .env.example .env

# Edit configuration
nano config/default.yaml
nano .env
```

### 4. Build TypeScript

```bash
npm run build
```

### 5. Start Argentum

**Production mode:**
```bash
npm start
```

**Development mode (with hot-reload):**
```bash
npm run dev
```

---

## Configuration

### Environment Variables (`.env`)

| Variable | Description | Required |
|---|---|---|
| `AGCLAW_TELEGRAM_TOKEN` | Telegram bot token from @BotFather | For Telegram |
| `OPENROUTER_API_KEY` | OpenRouter API key for model access | Yes |
| `ELEVENLABS_API_KEY` | ElevenLabs key for voice features | For voice |
| `AGCLAW_SUPABASE_URL` | Supabase project URL | For Supabase memory |
| `AGCLAW_SUPABASE_KEY` | Supabase anon/service key | For Supabase memory |
| `AGCLAW_FCM_KEY` | Firebase Cloud Messaging key | For mobile push |
| `AGCLAW_LOG_LEVEL` | Logging level (debug/info/warn/error) | No |
| `AGCLAW_PORT` | Override default port (18789) | No |

### YAML Configuration (`config/default.yaml`)

The main configuration file controls everything:

```yaml
# Server
server:
  port: 18789
  host: "0.0.0.0"

# Model
model:
  provider: openrouter
  defaultModel: "anthropic/claude-sonnet-4-20250514"

# Features (enable/disable)
features:
  webchat:
    enabled: true
  voice:
    enabled: false

# Memory backend
memory:
  primary: sqlite    # sqlite | supabase | markdown
  selfEvolving: true

# Security
security:
  allowlistMode: permissive   # permissive | strict
  auditLog: true
```

### Security Policy (`config/security-policy.yaml`)

Controls what commands, paths, and network hosts the agent can access. See the [Architecture Guide](architecture.md) for details.

---

## Updating

```bash
# Pull latest changes
git pull origin main

# Reinstall dependencies
npm install

# Rebuild
npm run build

# Restart
npm start
```

**Docker:**
```bash
docker compose -f docker/docker-compose.yml pull
docker compose -f docker/docker-compose.yml up -d --build
```

---

## Troubleshooting

### Port Already in Use

```bash
# Find what's using port 18789
lsof -i :18789

# Or change the port in config/default.yaml
server:
  port: 3000
```

### Permission Denied on install.sh

```bash
chmod +x install.sh
./install.sh
```

### TypeScript Build Errors

```bash
# Clean and rebuild
rm -rf dist/
npm run build
```

### Docker Permission Issues

```bash
# Add your user to docker group
sudo usermod -aG docker $USER
newgrp docker
```

### SQLite Database Locked

This happens when multiple processes try to access the database. Stop all instances and restart:

```bash
pkill -f "node.*argentum"
npm start
```

---

## Uninstalling

```bash
# Stop Argentum
# Ctrl+C or: docker compose down

# Remove data (careful!)
rm -rf data/ memory/ logs/

# Remove the project
cd .. && rm -rf argentum/
```
