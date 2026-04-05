/**
 * AG-Claw Dashboard - API Module
 * Handles all backend communication including WebSocket connections
 */

const API = {
  // Base configuration
  config: {
    baseUrl: '/api',
    /* nosemgrep: javascript.lang.security.detect-insecure-websocket.detect-insecure-websocket */
    wsUrl: `ws://${window.location.host}/ws`,
    reconnectDelay: 3000,
    maxRetries: 5,
  },

  // WebSocket connection
  ws: null,
  wsConnected: false,
  wsReconnectAttempts: 0,
  wsReconnectTimer: null,

  // Event handlers
  handlers: {
    onConnect: [],
    onDisconnect: [],
    onMessage: [],
    onError: [],
  },

  // Subscribed topics
  subscriptions: new Set(),

  /**
   * Initialize API module
   */
  init() {
    this.connectWebSocket();
    this.startHealthCheck();
    return this;
  },

  /**
   * Connect to WebSocket server
   */
  connectWebSocket() {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      return;
    }

    try {
      this.ws = new WebSocket(this.config.wsUrl);

      this.ws.onopen = () => {
        console.log('[API] WebSocket connected');
        this.wsConnected = true;
        this.wsReconnectAttempts = 0;
        this.emit('connect');
        this.resubscribeTopics();
      };

      this.ws.onclose = (event) => {
        console.log('[API] WebSocket disconnected', event.code);
        this.wsConnected = false;
        this.emit('disconnect', event);
        this.scheduleReconnect();
      };

      this.ws.onerror = (error) => {
        console.error('[API] WebSocket error:', error);
        this.emit('error', error);
      };

      this.ws.onmessage = (event) => {
        this.handleMessage(event.data);
      };
    } catch (error) {
      console.error('[API] Failed to connect WebSocket:', error);
      this.scheduleReconnect();
    }
  },

  /**
   * Schedule reconnection attempt
   */
  scheduleReconnect() {
    if (this.wsReconnectTimer) {
      clearTimeout(this.wsReconnectTimer);
    }

    if (this.wsReconnectAttempts >= this.config.maxRetries) {
      console.log('[API] Max reconnection attempts reached');
      return;
    }

    const delay = this.config.reconnectDelay * Math.pow(1.5, this.wsReconnectAttempts);
    console.log(`[API] Reconnecting in ${delay}ms (attempt ${this.wsReconnectAttempts + 1})`);

    this.wsReconnectTimer = setTimeout(() => {
      this.wsReconnectAttempts++;
      this.connectWebSocket();
    }, delay);
  },

  /**
   * Handle incoming WebSocket message
   */
  handleMessage(data) {
    try {
      const message = JSON.parse(data);

      // Route to appropriate handler
      if (message.topic && this.handlers.onMessage.length > 0) {
        this.emit('message', message);
      }
    } catch (error) {
      console.error('[API] Failed to parse message:', error);
    }
  },

  /**
   * Resubscribe to topics after reconnection
   */
  resubscribeTopics() {
    this.subscriptions.forEach((topic) => {
      this.send({ type: 'subscribe', topic });
    });
  },

  /**
   * Send message via WebSocket
   */
  send(data) {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(data));
      return true;
    }
    return false;
  },

  /**
   * Subscribe to a topic
   */
  subscribe(topic, callback) {
    this.subscriptions.add(topic);
    if (callback) {
      this.handlers.onMessage.push((msg) => {
        if (msg.topic === topic) {
          callback(msg.data);
        }
      });
    }
    if (this.wsConnected) {
      this.send({ type: 'subscribe', topic });
    }
    return this;
  },

  /**
   * Unsubscribe from a topic
   */
  unsubscribe(topic) {
    this.subscriptions.delete(topic);
    if (this.wsConnected) {
      this.send({ type: 'unsubscribe', topic });
    }
    return this;
  },

  /**
   * Register event handler
   */
  on(event, handler) {
    if (this.handlers[event]) {
      this.handlers[event].push(handler);
    }
    return this;
  },

  /**
   * Emit event
   */
  emit(event, ...args) {
    if (this.handlers[event]) {
      this.handlers[event].forEach((handler) => handler(...args));
    }
    return this;
  },

  /**
   * Start health check interval
   */
  startHealthCheck() {
    setInterval(() => {
      this.getHealth()
        .then((health) => {
          this.updateSystemStatus(health);
        })
        .catch(() => {
          // System might be initializing
        });
    }, 30000);
  },

  /**
   * Update system status in UI
   */
  updateSystemStatus(health) {
    // Dispatch custom event for UI components
    window.dispatchEvent(new CustomEvent('systemStatusUpdate', { detail: health }));
  },

  // =============================================================================
  // REST API Methods
  // =============================================================================

  /**
   * Make HTTP request
   */
  async request(endpoint, options = {}) {
    const url = `${this.config.baseUrl}${endpoint}`;
    const config = {
      headers: {
        'Content-Type': 'application/json',
        ...options.headers,
      },
      ...options,
    };

    try {
      const response = await fetch(url, config);

      if (!response.ok) {
        throw new Error(`HTTP ${response.status}: ${response.statusText}`);
      }

      return await response.json();
    } catch (error) {
      console.error(`[API] Request failed: ${endpoint} - ${error}`);
      throw error;
    }
  },

  /**
   * GET request
   */
  async get(endpoint) {
    return this.request(endpoint, { method: 'GET' });
  },

  /**
   * POST request
   */
  async post(endpoint, data) {
    return this.request(endpoint, {
      method: 'POST',
      body: JSON.stringify(data),
    });
  },

  /**
   * PUT request
   */
  async put(endpoint, data) {
    return this.request(endpoint, {
      method: 'PUT',
      body: JSON.stringify(data),
    });
  },

  /**
   * DELETE request
   */
  async delete(endpoint) {
    return this.request(endpoint, { method: 'DELETE' });
  },

  // =============================================================================
  // System API
  // =============================================================================

  /**
   * Get system health
   */
  getHealth() {
    return this.get('/health');
  },

  /**
   * Get system stats
   */
  getStats() {
    return this.get('/stats');
  },

  /**
   * Get system info
   */
  getSystemInfo() {
    return this.get('/system');
  },

  // =============================================================================
  // Agents API
  // =============================================================================

  /**
   * Get all agents
   */
  getAgents() {
    return this.get('/agents');
  },

  /**
   * Get single agent
   */
  getAgent(id) {
    return this.get(`/agents/${id}`);
  },

  /**
   * Create new agent
   */
  createAgent(data) {
    return this.post('/agents', data);
  },

  /**
   * Update agent
   */
  updateAgent(id, data) {
    return this.put(`/agents/${id}`, data);
  },

  /**
   * Delete agent
   */
  deleteAgent(id) {
    return this.delete(`/agents/${id}`);
  },

  /**
   * Start agent
   */
  startAgent(id) {
    return this.post(`/agents/${id}/start`);
  },

  /**
   * Stop agent
   */
  stopAgent(id) {
    return this.post(`/agents/${id}/stop`);
  },

  /**
   * Restart agent
   */
  restartAgent(id) {
    return this.post(`/agents/${id}/restart`);
  },

  /**
   * Get agent logs
   */
  getAgentLogs(id, options = {}) {
    const params = new URLSearchParams(options);
    return this.get(`/agents/${id}/logs?${params}`);
  },

  // =============================================================================
  // Skills API
  // =============================================================================

  /**
   * Get all skills
   */
  getSkills() {
    return this.get('/skills');
  },

  /**
   * Get skill details
   */
  getSkill(id) {
    return this.get(`/skills/${id}`);
  },

  /**
   * Install skill
   */
  installSkill(id) {
    return this.post(`/skills/${id}/install`);
  },

  /**
   * Uninstall skill
   */
  uninstallSkill(id) {
    return this.post(`/skills/${id}/uninstall`);
  },

  /**
   * Update skill
   */
  updateSkill(id) {
    return this.post(`/skills/${id}/update`);
  },

  // =============================================================================
  // Memory API
  // =============================================================================

  /**
   * Get memories
   */
  getMemories(type = 'all', options = {}) {
    const params = new URLSearchParams({ type, ...options });
    return this.get(`/memory?${params}`);
  },

  /**
   * Get single memory
   */
  getMemory(id) {
    return this.get(`/memory/${id}`);
  },

  /**
   * Create memory
   */
  createMemory(data) {
    return this.post('/memory', data);
  },

  /**
   * Update memory
   */
  updateMemory(id, data) {
    return this.put(`/memory/${id}`, data);
  },

  /**
   * Delete memory
   */
  deleteMemory(id) {
    return this.delete(`/memory/${id}`);
  },

  /**
   * Search memories
   */
  searchMemories(query) {
    return this.get(`/memory/search?q=${encodeURIComponent(query)}`);
  },

  // =============================================================================
  // Logs API
  // =============================================================================

  /**
   * Get logs
   */
  getLogs(options = {}) {
    const params = new URLSearchParams(options);
    return this.get(`/logs?${params}`);
  },

  /**
   * Get log stream (SSE)
   */
  streamLogs(options = {}) {
    const params = new URLSearchParams(options);
    return new EventSource(`${this.config.baseUrl}/logs/stream?${params}`);
  },

  /**
   * Export logs
   */
  exportLogs(options = {}) {
    const params = new URLSearchParams(options);
    window.open(`${this.config.baseUrl}/logs/export?${params}`, '_blank');
  },

  // =============================================================================
  // Settings API
  // =============================================================================

  /**
   * Get settings
   */
  getSettings() {
    return this.get('/settings');
  },

  /**
   * Update settings
   */
  updateSettings(data) {
    return this.put('/settings', data);
  },

  /**
   * Get API keys status
   */
  getApiKeyStatus() {
    return this.get('/settings/keys');
  },

  /**
   * Update API key
   */
  updateApiKey(name, value) {
    return this.put(`/settings/keys/${name}`, { value });
  },
};

// Make API globally available
window.API = API;
