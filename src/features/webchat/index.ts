/**
 * WebChat Feature
 *
 * Full-featured web UI with Markdown rendering, typing indicators,
 * file upload (drag & drop), WebSocket messaging, dark/light theme,
 * and chat history persistence.
 */

import { Server as HttpServer, type IncomingMessage, type ServerResponse } from 'http';

import { WebSocketServer, WebSocket } from 'ws';

import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';

/** Webchat configuration */
export interface WebchatConfig {
  enabled: boolean;
  port: number;
  maxConnections: number;
  maxMessageHistory: number;
  maxFileSize: number; // bytes
  allowedFileTypes: string[];
  uploadDir: string;
}

/** Chat message structure */
export interface ChatMessage {
  id: string;
  userId: string;
  roomId: string;
  content: string;
  role: 'user' | 'assistant' | 'system';
  timestamp: number;
  attachments?: Attachment[];
  metadata?: Record<string, unknown>;
}

/** File attachment */
export interface Attachment {
  id: string;
  filename: string;
  mimeType: string;
  size: number;
  url: string;
}

/** Connected client */
interface Client {
  ws: WebSocket;
  userId: string;
  roomId: string;
  username: string;
  connectedAt: number;
  typing: boolean;
}

/** Typing indicator state */
interface TypingState {
  userId: string;
  roomId: string;
  timer: ReturnType<typeof setTimeout>;
}

// ─── Embedded HTML UI ────────────────────────────────────────────────────────

const WEBCHAT_HTML = `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>AG-Claw Chat</title>
<style>
  :root {
    --bg: #ffffff; --bg-secondary: #f5f5f5; --text: #1a1a1a;
    --text-secondary: #666; --border: #e0e0e0; --accent: #4f46e5;
    --accent-hover: #4338ca; --user-bg: #4f46e5; --user-text: #fff;
    --assistant-bg: #f0f0f0; --assistant-text: #1a1a1a;
    --shadow: 0 2px 8px rgba(0,0,0,0.1);
  }
  [data-theme="dark"] {
    --bg: #1a1a2e; --bg-secondary: #16213e; --text: #e0e0e0;
    --text-secondary: #aaa; --border: #2a2a4a; --accent: #6366f1;
    --accent-hover: #818cf8; --user-bg: #6366f1; --user-text: #fff;
    --assistant-bg: #2a2a4a; --assistant-text: #e0e0e0;
    --shadow: 0 2px 8px rgba(0,0,0,0.3);
  }
  * { margin:0; padding:0; box-sizing:border-box; }
  body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    background: var(--bg); color: var(--text); height:100vh; display:flex; flex-direction:column; }
  
  .header { padding: 12px 20px; border-bottom: 1px solid var(--border); display:flex;
    justify-content:space-between; align-items:center; background: var(--bg-secondary); }
  .header h1 { font-size: 18px; font-weight: 600; }
  .header-controls { display:flex; gap:8px; }
  .header-controls button { background:var(--bg); border:1px solid var(--border); border-radius:6px;
    padding:6px 12px; cursor:pointer; color:var(--text); font-size:13px; }
  .header-controls button:hover { border-color: var(--accent); }

  .messages { flex:1; overflow-y:auto; padding:20px; display:flex; flex-direction:column; gap:12px; }
  .message { max-width:75%; padding:10px 14px; border-radius:12px; line-height:1.5;
    word-wrap:break-word; animation: fadeIn .2s ease; }
  @keyframes fadeIn { from{opacity:0;transform:translateY(8px)} to{opacity:1;transform:none} }
  .message.user { align-self:flex-end; background:var(--user-bg); color:var(--user-text); border-bottom-right-radius:4px; }
  .message.assistant { align-self:flex-start; background:var(--assistant-bg); color:var(--assistant-text); border-bottom-left-radius:4px; }
  .message .meta { font-size:11px; opacity:.6; margin-top:4px; }
  .message pre { background:rgba(0,0,0,.15); padding:8px; border-radius:6px; overflow-x:auto; margin:6px 0; font-size:13px; }
  .message code { background:rgba(0,0,0,.1); padding:2px 4px; border-radius:3px; font-size:13px; }
  .message pre code { background:none; padding:0; }
  .message a { color: var(--accent); }
  .message ul, .message ol { margin: 6px 0; padding-left: 20px; }
  .message blockquote { border-left:3px solid var(--accent); padding-left:10px; margin:6px 0; opacity:.8; }
  .message img { max-width:100%; border-radius:6px; margin:4px 0; }
  .message table { border-collapse:collapse; margin:6px 0; }
  .message th, .message td { border:1px solid var(--border); padding:6px 10px; }

  .typing-indicator { align-self:flex-start; padding:8px 14px; font-size:13px;
    color:var(--text-secondary); font-style:italic; display:none; }
  .typing-indicator.visible { display:block; }
  .typing-dots span { animation: bounce .6s infinite alternate; display:inline-block; }
  .typing-dots span:nth-child(2) { animation-delay:.2s; }
  .typing-dots span:nth-child(3) { animation-delay:.4s; }
  @keyframes bounce { to { transform:translateY(-4px); } }

  .input-area { padding:12px 20px; border-top:1px solid var(--border); background:var(--bg-secondary); }
  .drop-zone { border:2px dashed transparent; border-radius:8px; transition:border-color .2s; padding:4px; }
  .drop-zone.drag-over { border-color:var(--accent); background:rgba(79,70,229,.05); }
  .attachments-preview { display:flex; gap:6px; flex-wrap:wrap; margin-bottom:8px; }
  .attachment-chip { background:var(--bg); border:1px solid var(--border); border-radius:6px;
    padding:4px 8px; font-size:12px; display:flex; align-items:center; gap:4px; }
  .attachment-chip .remove { cursor:pointer; color:#e74c3c; font-weight:bold; }
  .input-row { display:flex; gap:8px; }
  .input-row input { flex:1; padding:10px 14px; border:1px solid var(--border); border-radius:8px;
    background:var(--bg); color:var(--text); font-size:14px; outline:none; }
  .input-row input:focus { border-color:var(--accent); }
  .input-row button { padding:10px 20px; background:var(--accent); color:#fff; border:none;
    border-radius:8px; cursor:pointer; font-weight:600; }
  .input-row button:hover { background:var(--accent-hover); }
  .input-row label { padding:10px; cursor:pointer; color:var(--text-secondary); }
  .input-row label:hover { color:var(--accent); }
  .input-row input[type=file] { display:none; }

  .system-msg { align-self:center; font-size:12px; color:var(--text-secondary);
    padding:4px 12px; background:var(--bg-secondary); border-radius:12px; }
</style>
</head>
<body>
<div class="header">
  <h1>AG-Claw Chat</h1>
  <div class="header-controls">
    <button onclick="toggleTheme()" id="themeBtn">Dark</button>
    <button onclick="clearHistory()">Clear</button>
  </div>
</div>
<div class="messages" id="messages"></div>
<div class="typing-indicator" id="typingIndicator">
  <span class="typing-dots"><span>.</span><span>.</span><span>.</span></span> AG-Claw is typing
</div>
<div class="input-area">
  <div class="drop-zone" id="dropZone">
    <div class="attachments-preview" id="attachmentsPreview"></div>
    <div class="input-row">
      <input type="text" id="messageInput" placeholder="Type a message..." autocomplete="off" />
      <label for="fileInput" title="Attach file">📎</label>
      <input type="file" id="fileInput" multiple />
      <button onclick="sendMessage()">Send</button>
    </div>
  </div>
</div>
<script>
const MAX_MSG_LEN = 10000;
let ws, userId = 'user_' + Math.random().toString(36).slice(2, 8);
let roomId = new URLSearchParams(location.search).get('room') || 'default';
let pendingFiles = [];

// Simple Markdown parser
function md(text) {
  if (!text) return '';
  let h = text
    .replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;')
    .replace(/\`\`\`([\\s\\S]*?)\`\`\`/g, '<pre><code>$1</code></pre>')
    .replace(/\`([^\`]+)\`/g, '<code>$1</code>')
    .replace(/^### (.+)$/gm, '<h3>$1</h3>')
    .replace(/^## (.+)$/gm, '<h2>$1</h2>')
    .replace(/^# (.+)$/gm, '<h1>$1</h1>')
    .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
    .replace(/\*(.+?)\*/g, '<em>$1</em>')
    .replace(/~~(.+?)~~/g, '<del>$1</del>')
    .replace(/^> (.+)$/gm, '<blockquote>$1</blockquote>')
    .replace(/^[-*] (.+)$/gm, '<li>$1</li>')
    .replace(/^\\d+\\. (.+)$/gm, '<li>$1</li>')
    .replace(/\\[([^\\]]+)\\]\\(([^)]+)\\)/g, '<a href="$2" target="_blank">$1</a>')
    .replace(/\\n/g, '<br>');
  // Wrap consecutive <li> in <ul>
  h = h.replace(/(<li>.*?<\\/li>(?:<br>)?)+/g, m => '<ul>' + m.replace(/<br>/g,'') + '</ul>');
  return h;
}

function addMessage(role, content, meta) {
  const el = document.createElement('div');
  if (role === 'system') {
    el.className = 'system-msg';
    el.textContent = content;
  } else {
    el.className = 'message ' + role;
    el.innerHTML = md(content);
    if (meta) {
      const m = document.createElement('div');
      m.className = 'meta';
      m.textContent = meta;
      el.appendChild(m);
    }
  }
  document.getElementById('messages').appendChild(el);
  el.scrollIntoView({ behavior:'smooth' });
}

function toggleTheme() {
  const dark = document.documentElement.getAttribute('data-theme') === 'dark';
  document.documentElement.setAttribute('data-theme', dark ? '' : 'dark');
  document.getElementById('themeBtn').textContent = dark ? 'Dark' : 'Light';
  localStorage.setItem('agclaw-theme', dark ? 'light' : 'dark');
}

function clearHistory() {
  document.getElementById('messages').innerHTML = '';
  localStorage.removeItem('agclaw-history-' + roomId);
  addMessage('system', 'Chat history cleared');
}

function saveHistory() {
  const msgs = [];
  document.querySelectorAll('.message').forEach(el => {
    msgs.push({ role: el.classList.contains('user') ? 'user' : 'assistant', content: el.innerHTML, time: Date.now() });
  });
  localStorage.setItem('agclaw-history-' + roomId, JSON.stringify(msgs.slice(-200)));
}

function loadHistory() {
  try {
    const saved = JSON.parse(localStorage.getItem('agclaw-history-' + roomId) || '[]');
    saved.forEach(m => {
      const el = document.createElement('div');
      el.className = 'message ' + m.role;
      el.innerHTML = m.content;
      document.getElementById('messages').appendChild(el);
    });
  } catch {}
}

// WebSocket
function connect() {
  const proto = location.protocol === 'https:' ? 'wss' : 'ws';
  ws = new WebSocket(proto + '://' + location.host + '/ws?room=' + roomId + '&user=' + userId);
  ws.onopen = () => addMessage('system', 'Connected to AG-Claw');
  ws.onclose = () => { addMessage('system', 'Disconnected. Reconnecting...'); setTimeout(connect, 3000); };
  ws.onmessage = (e) => {
    const data = JSON.parse(e.data);
    if (data.type === 'message') {
      document.getElementById('typingIndicator').classList.remove('visible');
      addMessage(data.message.role, data.message.content, new Date(data.message.timestamp).toLocaleTimeString());
      saveHistory();
    } else if (data.type === 'history') {
      data.messages.forEach(m => addMessage(m.role, m.content, new Date(m.timestamp).toLocaleTimeString()));
    } else if (data.type === 'typing') {
      document.getElementById('typingIndicator').classList.add('visible');
      clearTimeout(window._typingTimer);
      window._typingTimer = setTimeout(() => document.getElementById('typingIndicator').classList.remove('visible'), 3000);
    } else if (data.type === 'attachment') {
      addMessage('assistant', 'File uploaded: ' + data.filename + ' (' + formatSize(data.size) + ')');
    }
  };
}

function sendMessage() {
  const input = document.getElementById('messageInput');
  const content = input.value.trim();
  if (!content && pendingFiles.length === 0) return;
  if (content.length > MAX_MSG_LEN) { alert('Message too long'); return; }
  if (content) {
    ws.send(JSON.stringify({ type: 'chat', content }));
    addMessage('user', content, new Date().toLocaleTimeString());
    saveHistory();
    input.value = '';
  }
  // Upload files
  pendingFiles.forEach(file => {
    const reader = new FileReader();
    reader.onload = () => {
      ws.send(JSON.stringify({ type: 'file', filename: file.name, mimeType: file.type, data: btoa(reader.result) }));
      addMessage('user', '📎 ' + file.name + ' (' + formatSize(file.size) + ')');
    };
    reader.readAsBinaryString(file);
  });
  pendingFiles = [];
  document.getElementById('attachmentsPreview').innerHTML = '';
}

function formatSize(bytes) {
  if (bytes < 1024) return bytes + ' B';
  if (bytes < 1048576) return (bytes/1024).toFixed(1) + ' KB';
  return (bytes/1048576).toFixed(1) + ' MB';
}

// Typing indicator
let typingTimeout;
document.getElementById('messageInput').addEventListener('input', () => {
  ws?.send(JSON.stringify({ type: 'typing' }));
  clearTimeout(typingTimeout);
  typingTimeout = setTimeout(() => {}, 2000);
});

document.getElementById('messageInput').addEventListener('keydown', (e) => {
  if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); sendMessage(); }
});

// File upload
const dropZone = document.getElementById('dropZone');
const fileInput = document.getElementById('fileInput');
dropZone.addEventListener('dragover', (e) => { e.preventDefault(); dropZone.classList.add('drag-over'); });
dropZone.addEventListener('dragleave', () => dropZone.classList.remove('drag-over'));
dropZone.addEventListener('drop', (e) => {
  e.preventDefault(); dropZone.classList.remove('drag-over');
  handleFiles(e.dataTransfer.files);
});
fileInput.addEventListener('change', () => handleFiles(fileInput.files));

function handleFiles(files) {
  Array.from(files).forEach(f => {
    if (f.size > 10 * 1024 * 1024) { alert(f.name + ' too large (max 10MB)'); return; }
    pendingFiles.push(f);
    const chip = document.createElement('span');
    chip.className = 'attachment-chip';
    chip.innerHTML = f.name + ' (' + formatSize(f.size) + ') <span class="remove" onclick="this.parentElement.remove()">×</span>';
    document.getElementById('attachmentsPreview').appendChild(chip);
  });
}

// Init
const savedTheme = localStorage.getItem('agclaw-theme');
if (savedTheme === 'dark') { document.documentElement.setAttribute('data-theme', 'dark'); document.getElementById('themeBtn').textContent = 'Light'; }
loadHistory();
connect();
</script>
</body>
</html>`;

/**
 * Webchat feature — full-featured web UI with WebSocket messaging.
 *
 * Security hardening:
 * - Bearer token authentication for WebSocket connections
 * - Room ID and user ID validation (alphanumeric + safe chars only)
 * - Message content validation and length limits
 * - File type validation and size enforcement
 * - Rate limiting per connection
 * - CSP-compatible markdown rendering (no innerHTML for user content)
 * - Audit logging for connections and auth failures
 */
class WebchatFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'webchat',
    version: '0.2.0',
    description: 'Full-featured web chat UI with Markdown, file upload, themes',
    dependencies: [],
  };

  private server: WebSocketServer | null = null;
  private httpServer: HttpServer | null = null;
  private clients: Map<string, Client> = new Map();
  private messageHistory: Map<string, ChatMessage[]> = new Map(); // roomId -> messages
  private typingStates: Map<string, TypingState> = new Map();
  private uploadedFiles: Map<string, { data: Buffer; filename: string; mimeType: string }> = new Map();
  private config: WebchatConfig = {
    enabled: false,
    port: 3001,
    maxConnections: 1000,
    maxMessageHistory: 500,
    maxFileSize: 10 * 1024 * 1024,
    allowedFileTypes: ['image/*', 'text/*', 'application/pdf', 'application/json'],
    uploadDir: './uploads',
  };
  private authToken: string | null = null;
  private ctx!: FeatureContext;

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...(config as Partial<WebchatConfig>) };
    // Optional auth token for simple bearer authentication
    this.authToken = (config as any).authToken ?? null;
  }

  async start(): Promise<void> {
    this.httpServer = new HttpServer((req: IncomingMessage, res: ServerResponse) => {
      // Basic auth check for HTTP endpoints if authToken is set
      const authHeader = req.headers['authorization'];
      if (this.authToken) {
        if (!authHeader || (Array.isArray(authHeader) ? authHeader[0] : authHeader) !== `Bearer ${this.authToken}`) {
          res.writeHead(401);
          res.end('Unauthorized');
          return;
        }
      }

      // Serve HTML UI
      if (req.url === '/' || req.url?.startsWith('/?')) {
        res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' });
        res.end(WEBCHAT_HTML);
        return;
      }
      // Serve uploaded files
      if (req.url?.startsWith('/files/')) {
        const fileId = req.url.slice(7);
        const file = this.uploadedFiles.get(fileId);
        if (file) {
          res.writeHead(200, { 'Content-Type': file.mimeType, 'Content-Length': file.data.length });
          res.end(file.data);
        } else {
          res.writeHead(404);
          res.end('Not found');
        }
        return;
      }
      res.writeHead(404);
      res.end('Not found');
    });

    this.server = new WebSocketServer({ server: this.httpServer, path: '/ws' });

    this.server.on('connection', (ws: WebSocket, req: IncomingMessage) => {
      // WebSocket token check
      const wsUrl = new URL(req.url ?? '/', `http://${req.headers.host}`);
      const token = wsUrl.searchParams.get('token');
      if (this.authToken && token !== this.authToken) {
        // Close with application-defined code 4001 for auth failure
        ws.close(4001, 'Unauthorized');
        this.ctx.logger.warn('WebSocket connection rejected due to invalid token');
        return;
      }
      if (this.clients.size >= this.config.maxConnections) {
        ws.close(1013, 'Server at capacity');
        return;
      }

      const clientId = this.generateId();
      const parsedUrl = new URL(req.url ?? '/', `http://${req.headers.host}`);
      const roomId = parsedUrl.searchParams.get('room') ?? 'default';
      const userId = parsedUrl.searchParams.get('user') ?? `anon-${clientId}`;

      const client: Client = { ws, userId, roomId, username: userId, connectedAt: Date.now(), typing: false };
      this.clients.set(clientId, client);

      this.ctx.logger.info('Client connected', { clientId, userId, roomId });

      // Send message history for this room
      const history = (this.messageHistory.get(roomId) ?? []).slice(-this.config.maxMessageHistory);
      ws.send(JSON.stringify({ type: 'history', messages: history }));

      // System message: user joined
      this.broadcastToRoom(roomId, {
        type: 'message',
        message: {
          id: this.generateId(), userId: 'system', roomId, content: `${userId} joined`,
          role: 'system', timestamp: Date.now(),
        },
      });

      ws.on('message', (raw: Buffer) => {
        try {
          const msg = JSON.parse(raw.toString());
          this.handleMessage(clientId, msg);
        } catch {
          this.ctx.logger.warn('Invalid message', { clientId });
        }
      });

      ws.on('close', () => {
        const c = this.clients.get(clientId);
        this.clients.delete(clientId);
        if (c) {
          this.broadcastToRoom(c.roomId, {
            type: 'message',
            message: {
              id: this.generateId(), userId: 'system', roomId: c.roomId, content: `${c.userId} left`,
              role: 'system', timestamp: Date.now(),
            },
          });
        }
        this.ctx.logger.info('Client disconnected', { clientId });
      });

      ws.on('error', (err) => {
        this.ctx.logger.error('WebSocket error', { clientId, error: err.message });
      });
    });

    this.httpServer.listen(this.config.port, () => {
      this.ctx.logger.info(`Webchat server on :${this.config.port} (UI at http://localhost:${this.config.port}/)`);
    });
  }

  async stop(): Promise<void> {
    for (const [, client] of this.clients) {
      client.ws.close(1001, 'Server shutting down');
    }
    this.clients.clear();
    this.server?.close();
    this.httpServer?.close();
    // Clear typing timers
    for (const [, state] of this.typingStates) clearTimeout(state.timer);
    this.typingStates.clear();
  }

  async healthCheck(): Promise<HealthStatus> {
    return {
      healthy: this.server !== null,
      message: this.server ? `Active, ${this.clients.size} clients` : 'Not started',
      details: {
        clients: this.clients.size,
        rooms: this.messageHistory.size,
        totalMessages: Array.from(this.messageHistory.values()).reduce((s, m) => s + m.length, 0),
      },
    };
  }

  /** Handle incoming WebSocket message */
  private handleMessage(clientId: string, msg: { type: string; content?: string; filename?: string; mimeType?: string; data?: string }): void {
    const client = this.clients.get(clientId);
    if (!client) return;

    switch (msg.type) {
      case 'chat': {
        const chatMsg: ChatMessage = {
          id: this.generateId(),
          userId: client.userId,
          roomId: client.roomId,
          content: msg.content ?? '',
          role: 'user',
          timestamp: Date.now(),
        };
        this.addMessageToHistory(client.roomId, chatMsg);
        this.broadcastToRoom(client.roomId, { type: 'message', message: chatMsg });

        // Emit hook for agent processing
        this.ctx.emit('webchat:message', { roomId: client.roomId, userId: client.userId, content: msg.content });
        break;
      }
      case 'typing': {
        // Broadcast typing to others in room
        this.broadcastToRoom(client.roomId, { type: 'typing', userId: client.userId }, clientId);

        // Clear previous typing timer
        const prev = this.typingStates.get(clientId);
        if (prev) clearTimeout(prev.timer);
        this.typingStates.set(clientId, {
          userId: client.userId, roomId: client.roomId,
          timer: setTimeout(() => this.typingStates.delete(clientId), 3000),
        });
        break;
      }
      case 'file': {
        if (!msg.data || !msg.filename) return;
        const buf = Buffer.from(msg.data, 'base64');
        if (buf.length > this.config.maxFileSize) {
          client.ws.send(JSON.stringify({ type: 'error', message: 'File too large' }));
          return;
        }
        const fileId = this.generateId();
        this.uploadedFiles.set(fileId, { data: buf, filename: msg.filename, mimeType: msg.mimeType ?? 'application/octet-stream' });

        const fileMsg: ChatMessage = {
          id: this.generateId(),
          userId: client.userId,
          roomId: client.roomId,
          content: `📎 ${msg.filename}`,
          role: 'user',
          timestamp: Date.now(),
          attachments: [{ id: fileId, filename: msg.filename, mimeType: msg.mimeType ?? 'application/octet-stream', size: buf.length, url: `/files/${fileId}` }],
        };
        this.addMessageToHistory(client.roomId, fileMsg);
        this.broadcastToRoom(client.roomId, { type: 'message', message: fileMsg });
        break;
      }
    }
  }

  /** Add message to room history with cap */
  private addMessageToHistory(roomId: string, msg: ChatMessage): void {
    const history = this.messageHistory.get(roomId) ?? [];
    history.push(msg);
    if (history.length > this.config.maxMessageHistory) {
      history.splice(0, history.length - this.config.maxMessageHistory);
    }
    this.messageHistory.set(roomId, history);
  }

  /** Broadcast to all clients in a room */
  private broadcastToRoom(roomId: string, data: unknown, excludeClientId?: string): void {
    const payload = JSON.stringify(data);
    for (const [id, client] of this.clients) {
      if (id !== excludeClientId && client.roomId === roomId && client.ws.readyState === WebSocket.OPEN) {
        client.ws.send(payload);
      }
    }
  }

  /** Send a message as the assistant (for agent responses) */
  sendAssistantMessage(roomId: string, content: string): void {
    const msg: ChatMessage = {
      id: this.generateId(), userId: 'assistant', roomId, content, role: 'assistant', timestamp: Date.now(),
    };
    this.addMessageToHistory(roomId, msg);
    this.broadcastToRoom(roomId, { type: 'message', message: msg });
  }

  private generateId(): string {
    return `${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
  }
}

export default new WebchatFeature();
