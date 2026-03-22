/**
 * AG-Claw Dashboard - Logs Page
 */

// Log stream interval
let logStreamInterval = null;

// Initialize logs page
function initLogs() {
  const logsBody = document.getElementById('logsBody');
  if (!logsBody) return;

  // Generate initial mock logs
  generateInitialLogs();

  // Start live log stream
  startLogStream();
}

function generateInitialLogs() {
  const logsBody = document.getElementById('logsBody');
  if (!logsBody) return;

  const levels = ['DEBUG', 'INFO', 'INFO', 'INFO', 'WARN', 'ERROR'];
  const sources = ['core', 'agent:coder', 'agent:researcher', 'skills:github', 'channels:telegram', 'memory'];
  const messages = [
    'Processing incoming request from client',
    'Task completed successfully',
    'Memory query executed in 23ms',
    'Skill loaded: github integration',
    'API request completed in 145ms',
    'Cache invalidated for key: user_prefs',
    'Connection established to database',
    'Heartbeat received from agent',
    'Configuration updated via API',
    'Scheduled job executed: cleanup',
    'Authentication successful for user',
    'Rate limit check passed',
    'Webhook received from GitHub',
    'Message queued for delivery',
    'Session established with Telegram'
  ];

  const now = Date.now();
  const logs = [];

  // Generate 50 initial logs
  for (let i = 0; i < 50; i++) {
    const level = levels[Math.floor(Math.random() * levels.length)];
    const source = sources[Math.floor(Math.random() * sources.length)];
    const message = messages[Math.floor(Math.random() * messages.length)];
    const timestamp = new Date(now - (50 - i) * 3000).toTimeString().split(' ')[0];

    logs.push({ timestamp, level, source, message });
  }

  logsBody.innerHTML = logs.map(log => createLogLine(log)).join('');

  // Update count
  const countEl = document.getElementById('logCount');
  if (countEl) {
    countEl.textContent = `${logs.length} entries`;
  }
}

function startLogStream() {
  // Clear any existing stream
  if (logStreamInterval) {
    clearInterval(logStreamInterval);
  }

  // Generate new log entries every 2-5 seconds
  logStreamInterval = setInterval(() => {
    addLiveLog();
  }, 2000 + Math.random() * 3000);
}

function addLiveLog() {
  const levels = ['DEBUG', 'INFO', 'INFO', 'WARN'];
  const sources = ['core', 'agent:coder', 'agent:researcher', 'skills:github', 'memory'];
  const messages = [
    'Processing request',
    'Task completed',
    'Cache hit for query',
    'Skill executed successfully',
    'Heartbeat sent',
    'Connection pool check',
    'Background job started',
    'Request validated',
    'Response serialized',
    'Metrics collected'
  ];

  const level = levels[Math.floor(Math.random() * levels.length)];
  const source = sources[Math.floor(Math.random() * sources.length)];
  const message = messages[Math.floor(Math.random() * messages.length)];
  const timestamp = new Date().toTimeString().split(' ')[0];

  addLogEntry({ timestamp, level, source, message });
}

function createLogLine(log) {
  return `
    <div class="log-line">
      <span class="timestamp">${log.timestamp}</span>
      <span class="level ${log.level.toLowerCase()}">${log.level}</span>
      <span class="source">[${log.source}]</span>
      <span class="message">${log.message}</span>
    </div>
  `;
}

function addLogEntry(log) {
  const logsBody = document.getElementById('logsBody');
  if (!logsBody) return;

  const entry = document.createElement('div');
  entry.className = 'log-line';
  entry.innerHTML = createLogLine(log);
  entry.style.animation = 'fadeIn 0.2s ease forwards';

  logsBody.appendChild(entry);

  // Limit total logs to 200
  while (logsBody.children.length > 200) {
    logsBody.removeChild(logsBody.firstChild);
  }

  // Update count
  const countEl = document.getElementById('logCount');
  if (countEl) {
    countEl.textContent = `${logsBody.children.length} entries`;
  }

  // Auto scroll if enabled
  if (App.autoScroll) {
    scrollToBottom();
  }
}

function scrollToBottom() {
  const logsBody = document.getElementById('logsBody');
  if (logsBody) {
    requestAnimationFrame(() => {
      logsBody.scrollTop = logsBody.scrollHeight;
    });
  }
}

function filterLogsByLevel(level) {
  const logsBody = document.getElementById('logsBody');
  if (!logsBody) return;

  const lines = logsBody.querySelectorAll('.log-line');
  lines.forEach(line => {
    const levelSpan = line.querySelector('.level');
    if (levelSpan) {
      const lineLevel = levelSpan.classList[1]; // second class is the level
      if (level === 'all' || lineLevel === level) {
        line.style.display = '';
      } else {
        line.style.display = 'none';
      }
    }
  });
}

function searchLogs(query) {
  if (!query.trim()) return;

  const logsBody = document.getElementById('logsBody');
  if (!logsBody) return;

  const lowerQuery = query.toLowerCase();
  const lines = logsBody.querySelectorAll('.log-line');

  lines.forEach(line => {
    const text = line.textContent.toLowerCase();
    if (text.includes(lowerQuery)) {
      line.style.display = '';
      // Highlight matching text
      line.querySelectorAll('.message').forEach(msg => {
        const content = msg.textContent;
        if (content.toLowerCase().includes(lowerQuery)) {
          const regex = new RegExp(`(${query})`, 'gi');
          msg.innerHTML = content.replace(regex, '<mark style="background: var(--color-warning-muted); color: var(--color-warning);">$1</mark>');
        }
      });
    } else {
      line.style.display = 'none';
    }
  });
}

function exportLogsToFile() {
  const logsBody = document.getElementById('logsBody');
  if (!logsBody) return;

  const logs = Array.from(logsBody.querySelectorAll('.log-line')).map(line => {
    return line.textContent.trim().replace(/\s+/g, ' ');
  });

  const content = logs.join('\n');
  const filename = `ag-claw-logs-${new Date().toISOString().split('T')[0]}.txt`;

  Components.downloadAsFile(content, filename, 'text/plain');
  Components.toast('Logs exported successfully', 'success');
}

function clearAllLogs() {
  const logsBody = document.getElementById('logsBody');
  if (!logsBody) return;

  logsBody.innerHTML = '';
  
  const countEl = document.getElementById('logCount');
  if (countEl) {
    countEl.textContent = '0 entries';
  }
}

// Initialize
document.addEventListener('DOMContentLoaded', () => {
  // Logs will be initialized when page is navigated to
});
