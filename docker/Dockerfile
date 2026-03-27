# ============================================
# Stage 1: Build
# ============================================
FROM node:20-alpine AS builder

WORKDIR /app

# Copy dependency manifests first (cache layer, updated 2026-03-27)
COPY package.json package-lock.json* ./
RUN npm ci --ignore-scripts --legacy-peer-deps

# Copy source and build
COPY tsconfig.json ./
COPY src/ ./src/
RUN npm run build

# Prune dev dependencies
RUN npm prune --production

# ============================================
# Stage 2: Production
# ============================================
FROM node:20-alpine

# Install wget for health checks (smaller than curl)
RUN apk add --no-cache wget

# Create non-root user
RUN addgroup -g 1001 -S agclaw && \
    adduser -u 1001 -S agclaw -G agclaw

WORKDIR /app

# Copy built artifacts from builder
COPY --from=builder --chown=agclaw:agclaw /app/dist ./dist
COPY --from=builder --chown=agclaw:agclaw /app/node_modules ./node_modules
COPY --from=builder --chown=agclaw:agclaw /app/package.json ./

# Copy config directory
COPY --chown=agclaw:agclaw config/ ./config/

# Create writable directories
RUN mkdir -p data memory logs && \
    chown -R agclaw:agclaw /app

# Switch to non-root user
USER agclaw

# Ports:
#   3000 - AG-Claw gateway HTTP
#   3001 - Webchat WebSocket
EXPOSE 3000 3001

# Health check against the gateway
HEALTHCHECK --interval=30s --timeout=10s --start-period=30s --retries=3 \
  CMD wget -qO- http://localhost:3000/health || exit 1

# Run
CMD ["node", "dist/index.js"]
