#!/usr/bin/env bash
# ============================================
# Argentum Installer
# ============================================
set -euo pipefail

# ─── Colors ──────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

# ─── Defaults ────────────────────────────────
INSTALL_DIR="${AGCLAW_DIR:-$(pwd)}"
USE_DOCKER=false

# ─── Parse args ──────────────────────────────
for arg in "$@"; do
  case $arg in
    --docker)    USE_DOCKER=true; shift ;;
    --dir=*)     INSTALL_DIR="${arg#*=}"; shift ;;
    --help|-h)
      echo -e "${BOLD}Argentum Installer${NC}"
      echo ""
      echo "Usage: ./install.sh [OPTIONS]"
      echo ""
      echo "Options:"
      echo "  --docker       Deploy with Docker Compose"
      echo "  --dir=PATH     Install directory (default: current)"
      echo "  --help         Show this help"
      echo ""
      echo "Env vars:"
      echo "  AGCLAW_DIR     Installation directory"
      exit 0
      ;;
  esac
done

# ─── Helpers ─────────────────────────────────
log_info()  { echo -e "${BLUE}[INFO]${NC} $1"; }
log_ok()    { echo -e "${GREEN}[  OK]${NC} $1"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERR ]${NC} $1"; }
log_step()  { echo -e "${BOLD}${BLUE}==> $1${NC}"; }

# ─── Banner ──────────────────────────────────
echo ""
echo -e "${BLUE}╔═══════════════════════════════════════╗${NC}"
echo -e "${BLUE}║        Argentum Installer v0.0.5       ║${NC}"
echo -e "${BLUE}║    Modular AI Agent Framework         ║${NC}"
echo -e "${BLUE}╚═══════════════════════════════════════╝${NC}"
echo ""

# ============================================
# Step 1: Prerequisites
# ============================================
log_step "Checking prerequisites"

# Node.js >= 20
if command -v node &>/dev/null; then
  NODE_VER=$(node -v | sed 's/v//')
  NODE_MAJOR=$(echo "$NODE_VER" | cut -d. -f1)
  if [ "$NODE_MAJOR" -ge 20 ]; then
    log_ok "Node.js v${NODE_VER}"
  else
    log_error "Node.js >= 20 required (found v${NODE_VER})"
    exit 1
  fi
else
  log_error "Node.js not found"
  log_info "Install: https://nodejs.org or use nvm/fnm"
  exit 1
fi

# npm
if command -v npm &>/dev/null; then
  log_ok "npm v$(npm -v)"
else
  log_error "npm not found"
  exit 1
fi

# Docker (only needed for --docker)
if [ "$USE_DOCKER" = true ]; then
  if command -v docker &>/dev/null; then
    DOCKER_VER=$(docker --version | grep -oP '\d+\.\d+\.\d+' | head -1)
    log_ok "Docker ${DOCKER_VER}"
  else
    log_error "Docker not found. Install: https://docs.docker.com/get-docker/"
    exit 1
  fi

  if docker compose version &>/dev/null || command -v docker-compose &>/dev/null; then
    log_ok "Docker Compose available"
  else
    log_error "Docker Compose not found"
    exit 1
  fi
fi

# ============================================
# Step 2: Navigate to directory
# ============================================
log_step "Setting up directory"

if [ ! -d "$INSTALL_DIR" ]; then
  mkdir -p "$INSTALL_DIR"
  log_info "Created: $INSTALL_DIR"
fi

cd "$INSTALL_DIR"
log_ok "Working in: $(pwd)"

# ============================================
# Step 3: Install dependencies
# ============================================
log_step "Installing dependencies"

if [ -f "package-lock.json" ]; then
  npm ci --ignore-scripts 2>/dev/null || npm install
else
  npm install
fi
log_ok "Dependencies installed"

# ============================================
# Step 4: Configuration
# ============================================
log_step "Configuring"

# Default config
if [ -f "config/default.yaml" ]; then
  log_ok "config/default.yaml exists"
else
  log_warn "config/default.yaml missing — check repo"
fi

# Security policy
if [ -f "config/security-policy.yaml" ]; then
  log_ok "config/security-policy.yaml exists"
else
  log_warn "config/security-policy.yaml missing — check repo"
fi

# .env file
if [ ! -f ".env" ]; then
  cat > .env << 'EOF'
# Argentum Environment Variables
# ──────────────────────────────

# Telegram Bot (get token from @BotFather)
# AGCLAW_TELEGRAM_TOKEN=

# OpenRouter API (unified model access)
# OPENROUTER_API_KEY=

# ElevenLabs (voice features)
# ELEVENLABS_API_KEY=

# Master key for encrypted secrets
# AGCLAW_MASTER_KEY=

# Supabase (optional cloud memory)
# AGCLAW_SUPABASE_URL=
# AGCLAW_SUPABASE_KEY=

# Logging
AGCLAW_LOG_LEVEL=info

# Port
# AGCLAW_PORT=18789
EOF
  log_ok ".env template created"
else
  log_ok ".env already exists"
fi

# ============================================
# Step 5: Create directories
# ============================================
log_step "Creating data directories"

mkdir -p data memory logs data/workspace
log_ok "data/, memory/, logs/, data/workspace/ ready"

# ============================================
# Step 6: Build
# ============================================
log_step "Building TypeScript"

if npm run build 2>/dev/null; then
  log_ok "Build complete"
else
  log_warn "Build had errors — run 'npm run build' to see details"
fi

# ============================================
# Step 7: Docker deployment (optional)
# ============================================
if [ "$USE_DOCKER" = true ]; then
  log_step "Starting Docker deployment"

  if docker compose -f docker/docker-compose.yml up -d 2>/dev/null || \
     docker-compose -f docker/docker-compose.yml up -d 2>/dev/null; then
    log_ok "Argentum running in Docker"
    echo ""
    log_info "Gateway: http://localhost:18789"
    log_info "Webchat: http://localhost:3001"
    log_info "Logs:    docker compose -f docker/docker-compose.yml logs -f"
    log_info "Stop:    docker compose -f docker/docker-compose.yml down"
  else
    log_error "Docker Compose failed"
    exit 1
  fi
else
  echo ""
  echo -e "${GREEN}${BOLD}Installation complete!${NC}"
  echo ""
  echo -e "${BOLD}Next steps:${NC}"
  echo -e "  1. Edit ${YELLOW}.env${NC} with your API keys"
  echo -e "  2. Review ${YELLOW}config/default.yaml${NC}"
  echo -e "  3. Run Argentum:"
  echo -e "     ${GREEN}npm start${NC}              production"
  echo -e "     ${GREEN}npm run dev${NC}            development"
  echo -e "     ${GREEN}./install.sh --docker${NC}  Docker"
  echo ""
fi
