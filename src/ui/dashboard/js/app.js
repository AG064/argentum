/**
 * Argentum Dashboard - Main Application
 */

// Global state
const App = {
  currentPage: 'overview',
  sidebarOpen: true,
  autoScroll: true,
  theme: 'dark',
  charts: {},
};

// Initialize application
document.addEventListener('DOMContentLoaded', () => {
  initApp();
});

async function initApp() {
  console.log('[Argentum] Initializing dashboard...');

  // Setup event listeners
  setupNavigation();
  setupThemeToggle();
  setupMobileMenu();
  setupKeyboardShortcuts();
  setupSearch();

  // Initialize API
  API.init();

  // Load initial data
  await loadDashboardData();

  // Start real-time updates
  startRealTimeUpdates();

  console.log('[Argentum] Dashboard initialized');
}

// =============================================================================
// Navigation
// =============================================================================

function setupNavigation() {
  // Sidebar nav items
  document.querySelectorAll('.nav-item[data-page]').forEach((item) => {
    item.addEventListener('click', () => {
      const page = item.dataset.page;
      navigateTo(page);
    });
  });

  // Settings nav items
  document.querySelectorAll('.settings-nav-item').forEach((item) => {
    item.addEventListener('click', () => {
      const section = item.dataset.section;
      navigateToSettingsSection(section);
    });
  });

  // Memory type filters
  document.querySelectorAll('.memory-type').forEach((type) => {
    type.addEventListener('click', () => {
      document.querySelectorAll('.memory-type').forEach((t) => t.classList.remove('active'));
      type.classList.add('active');
      filterMemories(type.dataset.type);
    });
  });

  // Log level filters
  document.querySelectorAll('.log-level-btn').forEach((btn) => {
    btn.addEventListener('click', () => {
      document.querySelectorAll('.log-level-btn').forEach((b) => b.classList.remove('active'));
      btn.classList.add('active');
      filterLogs(btn.dataset.level);
    });
  });
}

function navigateTo(page) {
  // Update nav items
  document.querySelectorAll('.nav-item').forEach((item) => {
    item.classList.toggle('active', item.dataset.page === page);
  });

  // Update pages
  document.querySelectorAll('.page').forEach((p) => {
    p.classList.toggle('active', p.id === `page-${page}`);
  });

  // Update breadcrumb
  const breadcrumb = document.getElementById('breadcrumbCurrent');
  if (breadcrumb) {
    breadcrumb.textContent = page.charAt(0).toUpperCase() + page.slice(1);
  }

  App.currentPage = page;

  // Trigger page-specific initialization
  if (page === 'agents') loadAgents();
  if (page === 'skills') loadSkills();
  if (page === 'memory') loadMemories();
  if (page === 'logs') startLogStream();
  if (page === 'knowledge-graph') {
    // Initialize 3D knowledge graph when page loads
    if (window.KnowledgeGraph3D) {
      window.KnowledgeGraph3D.init('kg3dContainer');
    }
  }

  // Close mobile menu
  closeMobileMenu();
}

function navigateToSettingsSection(section) {
  document.querySelectorAll('.settings-nav-item').forEach((item) => {
    item.classList.toggle('active', item.dataset.section === section);
  });

  const settingsSection = document.getElementById(`settings-${section}`);
  if (settingsSection) {
    settingsSection.scrollIntoView({ behavior: 'smooth', block: 'start' });
  }
}

// =============================================================================
// Theme
// =============================================================================

function setupThemeToggle() {
  const themeToggle = document.getElementById('themeToggle');
  if (themeToggle) {
    themeToggle.addEventListener('click', toggleTheme);
  }

  // Load saved theme
  const savedTheme = localStorage.getItem('ag-claw-theme') || 'dark';
  setTheme(savedTheme);
}

function toggleTheme() {
  const newTheme = App.theme === 'dark' ? 'light' : 'dark';
  setTheme(newTheme);
}

function setTheme(theme) {
  App.theme = theme;
  document.documentElement.dataset.theme = theme;
  localStorage.setItem('ag-claw-theme', theme);

  // Update theme toggle icon
  const themeToggle = document.getElementById('themeToggle');
  if (themeToggle) {
    themeToggle.innerHTML =
      theme === 'dark'
        ? '<svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/></svg>'
        : '<svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="5"/><line x1="12" y1="1" x2="12" y2="3"/><line x1="12" y1="21" x2="12" y2="23"/><line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/><line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/><line x1="1" y1="12" x2="3" y2="12"/><line x1="21" y1="12" x2="23" y2="12"/><line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/><line x1="18.36" y1="5.64" x2="19.78" y2="4.22"/></svg>';
  }

  // Update charts for theme
  updateChartsTheme();
}

// =============================================================================
// Mobile Menu
// =============================================================================

function setupMobileMenu() {
  const mobileMenuBtn = document.getElementById('mobileMenuBtn');
  if (mobileMenuBtn) {
    mobileMenuBtn.addEventListener('click', toggleMobileMenu);
  }

  // Close on outside click
  document.addEventListener('click', (e) => {
    const sidebar = document.getElementById('sidebar');
    if (sidebar && !sidebar.contains(e.target) && !mobileMenuBtn?.contains(e.target)) {
      closeMobileMenu();
    }
  });
}

function toggleMobileMenu() {
  const sidebar = document.getElementById('sidebar');
  if (sidebar) {
    sidebar.classList.toggle('open', !App.sidebarOpen);
    App.sidebarOpen = !App.sidebarOpen;
  }
}

function closeMobileMenu() {
  const sidebar = document.getElementById('sidebar');
  if (sidebar && window.innerWidth <= 768) {
    sidebar.classList.remove('open');
    App.sidebarOpen = false;
  }
}

// =============================================================================
// Keyboard Shortcuts
// =============================================================================

function setupKeyboardShortcuts() {
  document.addEventListener('keydown', (e) => {
    // Command/Ctrl + key shortcuts
    if (e.metaKey || e.ctrlKey) {
      switch (e.key.toLowerCase()) {
        case 'k':
          e.preventDefault();
          document.getElementById('globalSearch')?.focus();
          break;
        case 'd':
          e.preventDefault();
          navigateTo('overview');
          break;
        case 'l':
          e.preventDefault();
          navigateTo('logs');
          break;
        case ',':
          e.preventDefault();
          navigateTo('settings');
          break;
        case '1':
          e.preventDefault();
          navigateTo('overview');
          break;
        case '2':
          e.preventDefault();
          navigateTo('agents');
          break;
        case '3':
          e.preventDefault();
          navigateTo('skills');
          break;
        case '4':
          e.preventDefault();
          navigateTo('memory');
          break;
      }
    }

    // Escape to close panels
    if (e.key === 'Escape') {
      closeAgentDetail();
      closeMobileMenu();
    }
  });
}

// =============================================================================
// Search
// =============================================================================

function setupSearch() {
  const globalSearch = document.getElementById('globalSearch');
  if (globalSearch) {
    globalSearch.addEventListener(
      'input',
      Components.debounce((e) => {
        handleGlobalSearch(e.target.value);
      }, 300),
    );

    globalSearch.addEventListener('keydown', (e) => {
      if (e.key === 'Enter') {
        executeGlobalSearch(e.target.value);
      }
    });
  }
}

function handleGlobalSearch(query) {
  if (query.length < 2) return;
  // Could implement search suggestions here
}

function executeGlobalSearch(query) {
  if (!query.trim()) return;
  console.log('Searching for:', query);
  // Could implement full search here
}

// =============================================================================
// Data Loading
// =============================================================================

async function loadDashboardData() {
  try {
    // Load system stats
    updateSystemStats();

    // Load agents
    loadAgents();

    // Load skills
    loadSkills();

    // Initialize charts
    initCharts();
  } catch (error) {
    console.error('[App] Failed to load dashboard data:', error);
  }
}

async function updateSystemStats() {
  // Simulated stats - in production, these would come from the API
  const stats = {
    uptime: '99.9%',
    activeAgents: 4,
    cpuUsage: Math.floor(Math.random() * 40) + 10,
    memoryUsage: (Math.random() * 2 + 0.5).toFixed(1) + ' GB',
  };

  // Update UI
  const uptimeEl = document.getElementById('uptime');
  if (uptimeEl) uptimeEl.textContent = stats.uptime;

  const agentsEl = document.getElementById('activeAgents');
  if (agentsEl) agentsEl.textContent = stats.activeAgents;

  const cpuEl = document.getElementById('cpuUsage');
  if (cpuEl) cpuEl.textContent = stats.cpuUsage + '%';

  const memoryEl = document.getElementById('memoryUsage');
  if (memoryEl) memoryEl.textContent = stats.memoryUsage;
}

function refreshDashboard() {
  loadDashboardData();
  Components.toast('Dashboard refreshed', 'success');
}

// =============================================================================
// Real-time Updates
// =============================================================================

function startRealTimeUpdates() {
  // Update system stats every 30 seconds
  setInterval(updateSystemStats, 30000);

  // Subscribe to system events
  API.subscribe('system:stats', (data) => {
    updateSystemStatsFromData(data);
  });

  API.subscribe('activity:new', (data) => {
    addActivityItem(data);
  });

  // Listen for system status updates
  window.addEventListener('systemStatusUpdate', (e) => {
    updateSystemStatsFromData(e.detail);
  });
}

function updateSystemStatsFromData(data) {
  // Update individual stat elements
  Object.entries(data).forEach(([key, value]) => {
    const el = document.getElementById(key);
    if (el) {
      el.textContent = value;
    }
  });
}

function addActivityItem(data) {
  const activityList = document.getElementById('activityList');
  if (!activityList) return;

  const item = document.createElement('div');
  item.className = 'activity-item';
  item.style.animation = 'fadeInUp 0.3s ease forwards';
  /* nosemgrep: javascript.browser.security.insecure-document-method.insecure-document-method */
  item.innerHTML = `
    <div class="activity-icon" style="background: var(--color-accent-muted); color: var(--color-accent);">
      ${data.icon || '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/></svg>'}
    </div>
    <div class="activity-content">
      <div class="activity-message">${data.message}</div>
      <div class="activity-time">just now</div>
    </div>
  `;

  // Insert at top
  activityList.insertBefore(item, activityList.firstChild);

  // Remove old items if too many
  while (activityList.children.length > 10) {
    activityList.removeChild(activityList.lastChild);
  }
}

// =============================================================================
// Charts
// =============================================================================

function initCharts() {
  initPerformanceChart();
  initAgentChart();
}

function initPerformanceChart() {
  const canvas = document.getElementById('performanceChart');
  if (!canvas) return;

  const ctx = canvas.getContext('2d');
  const isDark = App.theme === 'dark';

  // Gradient fill
  const gradient = ctx.createLinearGradient(0, 0, 0, 200);
  gradient.addColorStop(0, 'rgba(99, 102, 241, 0.3)');
  gradient.addColorStop(1, 'rgba(99, 102, 241, 0)');

  // Generate mock data
  const labels = [];
  const data = [];
  const now = Date.now();
  for (let i = 24; i >= 0; i--) {
    labels.push(`${i}h`);
    data.push(Math.random() * 50 + 20);
  }

  App.charts.performance = {
    type: 'line',
    data: {
      labels,
      datasets: [
        {
          data,
          borderColor: '#6366f1',
          backgroundColor: gradient,
          fill: true,
          tension: 0.4,
          pointRadius: 0,
          pointHoverRadius: 6,
          borderWidth: 2,
        },
      ],
    },
    options: {
      responsive: true,
      maintainAspectRatio: false,
      plugins: {
        legend: { display: false },
        tooltip: {
          backgroundColor: isDark ? '#1c1e2a' : '#ffffff',
          titleColor: isDark ? '#f0f2f5' : '#111827',
          bodyColor: isDark ? '#8b919e' : '#6b7280',
          borderColor: isDark ? 'rgba(255,255,255,0.1)' : 'rgba(0,0,0,0.1)',
          borderWidth: 1,
          padding: 12,
          displayColors: false,
        },
      },
      scales: {
        x: {
          grid: { display: false },
          ticks: {
            color: isDark ? '#5c6170' : '#9ca3af',
            font: { size: 10 },
          },
          border: { display: false },
        },
        y: {
          grid: {
            color: isDark ? 'rgba(255,255,255,0.05)' : 'rgba(0,0,0,0.05)',
          },
          ticks: {
            color: isDark ? '#5c6170' : '#9ca3af',
            font: { size: 10 },
            callback: (value) => value + '%',
          },
          border: { display: false },
          min: 0,
          max: 100,
        },
      },
      interaction: {
        intersect: false,
        mode: 'index',
      },
    },
  };

  // Simple chart implementation (no external library)
  drawSimpleChart(ctx, App.charts.performance);
}

function initAgentChart() {
  const canvas = document.getElementById('agentChart');
  if (!canvas) return;

  const ctx = canvas.getContext('2d');
  const isDark = App.theme === 'dark';

  // Generate mock data
  const labels = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun'];
  const coderData = [12, 19, 15, 22, 18, 14, 20];
  const researcherData = [8, 12, 14, 11, 16, 13, 15];
  const foremanData = [4, 6, 5, 7, 8, 6, 5];

  const drawBar = (x, y, width, height, color) => {
    ctx.fillStyle = color;
    ctx.beginPath();
    ctx.roundRect(x, y, width, height, 4);
    ctx.fill();
  };

  const chartWidth = canvas.width;
  const chartHeight = canvas.height;
  const barGroupWidth = chartWidth / labels.length;
  const barWidth = (barGroupWidth - 40) / 3;
  const gap = 8;

  labels.forEach((label, i) => {
    const groupX = i * barGroupWidth + 20;
    const baseY = chartHeight - 20;

    // Stack bars
    drawBar(groupX, baseY - coderData[i] * 3, barWidth, coderData[i] * 3, '#6366f1');
    drawBar(
      groupX + barWidth + gap / 2,
      baseY - researcherData[i] * 3,
      barWidth,
      researcherData[i] * 3,
      '#22c55e',
    );
    drawBar(
      groupX + (barWidth + gap / 2) * 2,
      baseY - foremanData[i] * 3,
      barWidth,
      foremanData[i] * 3,
      '#f59e0b',
    );
  });
}

function drawSimpleChart(ctx, config) {
  const { data, options } = config;
  const padding = { top: 20, right: 20, bottom: 30, left: 50 };
  const chartWidth = ctx.canvas.width - padding.left - padding.right;
  const chartHeight = ctx.canvas.height - padding.top - padding.bottom;

  ctx.clearRect(0, 0, ctx.canvas.width, ctx.canvas.height);

  const maxValue = Math.max(...data.datasets[0].data) * 1.1;
  const minValue = 0;

  // Draw grid lines
  ctx.strokeStyle = options.scales.y.grid.color;
  ctx.lineWidth = 1;
  for (let i = 0; i <= 4; i++) {
    const y = padding.top + (chartHeight / 4) * i;
    ctx.beginPath();
    ctx.moveTo(padding.left, y);
    ctx.lineTo(padding.left + chartWidth, y);
    ctx.stroke();
  }

  // Draw data
  const points = data.datasets[0].data.map((value, index) => {
    const x = padding.left + (index / (data.labels.length - 1)) * chartWidth;
    const y = padding.top + (1 - (value - minValue) / (maxValue - minValue)) * chartHeight;
    return { x, y };
  });

  // Draw area
  ctx.fillStyle = data.datasets[0].backgroundColor;
  ctx.beginPath();
  ctx.moveTo(points[0].x, padding.top + chartHeight);
  points.forEach((p) => ctx.lineTo(p.x, p.y));
  ctx.lineTo(points[points.length - 1].x, padding.top + chartHeight);
  ctx.closePath();
  ctx.fill();

  // Draw line
  ctx.strokeStyle = data.datasets[0].borderColor;
  ctx.lineWidth = 2;
  ctx.beginPath();
  points.forEach((p, i) => {
    if (i === 0) ctx.moveTo(p.x, p.y);
    else ctx.lineTo(p.x, p.y);
  });
  ctx.stroke();
}

function updateChartsTheme() {
  // Reinitialize charts with new theme colors
  initCharts();
}

// =============================================================================
// Agent Detail Panel
// =============================================================================

function openAgentDetail(agentId) {
  const panel = document.getElementById('agentDetail');
  const content = document.getElementById('agentDetailContent');

  if (!panel || !content) return;

  // Load agent details
  /* nosemgrep: javascript.browser.security.insecure-document-method.insecure-document-method */
  content.innerHTML = `
    <div class="flex justify-center p-8">
      ${Components.spinner('lg')}
    </div>
  `;

  panel.classList.add('open');

  // Simulated agent data
  setTimeout(() => {
    /* nosemgrep: javascript.browser.security.insecure-document-method.insecure-document-method */
    content.innerHTML = `
      <div class="agent-card" style="border: none; padding: 0;">
        <div class="agent-header">
          <div class="agent-avatar">🤖</div>
          <div class="agent-info">
            <div class="agent-name">Agent ${agentId}</div>
            <div class="agent-role">AI Assistant</div>
            <div class="agent-status">
              ${Components.statusDot('online')}
              Running
            </div>
          </div>
        </div>
        
        <div class="agent-stats">
          <div class="agent-stat">
            <span class="agent-stat-value">1,247</span>
            <span class="agent-stat-label">Tasks Completed</span>
          </div>
          <div class="agent-stat">
            <span class="agent-stat-value">99.8%</span>
            <span class="agent-stat-label">Success Rate</span>
          </div>
          <div class="agent-stat">
            <span class="agent-stat-value">2.3h</span>
            <span class="agent-stat-label">Avg Runtime</span>
          </div>
        </div>
        
        <div style="margin-bottom: var(--space-4);">
          <h4 style="font-size: var(--font-size-sm); font-weight: var(--font-weight-semibold); margin-bottom: var(--space-2);">Recent Logs</h4>
          <div class="agent-logs">
            <div class="log-entry">
              <span class="log-timestamp">12:34:56</span>
              <span class="log-level info">INFO</span>
              <span class="log-message">Agent initialized successfully</span>
            </div>
            <div class="log-entry">
              <span class="log-timestamp">12:35:02</span>
              <span class="log-level debug">DEBUG</span>
              <span class="log-message">Loading skill: coder</span>
            </div>
            <div class="log-entry">
              <span class="log-timestamp">12:35:05</span>
              <span class="log-level info">INFO</span>
              <span class="log-message">Ready to process tasks</span>
            </div>
          </div>
        </div>
        
        <div class="agent-actions">
          <button class="btn btn-secondary btn-sm flex-1">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <polygon points="5 3 19 12 5 21 5 3"/>
            </svg>
            Start
          </button>
          <button class="btn btn-warning btn-sm flex-1">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="6" y="4" width="4" height="16"/>
              <rect x="14" y="4" width="4" height="16"/>
            </svg>
            Pause
          </button>
          <button class="btn btn-danger btn-sm flex-1">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="3" y="3" width="18" height="18" rx="2" ry="2"/>
            </svg>
            Stop
          </button>
        </div>
      </div>
    `;
  }, 500);
}

function closeAgentDetail() {
  const panel = document.getElementById('agentDetail');
  if (panel) {
    panel.classList.remove('open');
  }
}

// =============================================================================
// Modals
// =============================================================================

function showModal(type) {
  switch (type) {
    case 'newAgent':
      Components.alert(
        'Create Agent',
        'Agent creation form would go here. Configure name, role, and permissions.',
      );
      break;
    case 'newSkill':
      Components.alert('Add Skill', 'Skill search and installation would go here.');
      break;
    case 'newMemory':
      Components.alert('Create Memory', 'Memory creation form would go here.');
      break;
  }
}

// =============================================================================
// Logs
// =============================================================================

let logStream = null;

function startLogStream() {
  const logsBody = document.getElementById('logsBody');
  if (!logsBody) return;

  // Generate initial mock logs
  generateMockLogs();

  // Simulate live log updates
  if (!logStream) {
    logStream = setInterval(() => {
      addMockLog();
    }, 2000);
  }
}

function generateMockLogs() {
  const logsBody = document.getElementById('logsBody');
  if (!logsBody) return;

  const levels = ['debug', 'info', 'info', 'info', 'warn', 'error'];
  const sources = ['core', 'agent:coder', 'agent:researcher', 'skills:github', 'channels:telegram'];
  const messages = [
    'Processing incoming request',
    'Task completed successfully',
    'Memory query executed',
    'Skill loaded: github',
    'API request completed',
    'Cache invalidated',
    'Connection established',
    'Heartbeat received',
    'Configuration updated',
    'Scheduled job executed',
  ];

  logsBody.innerHTML = '';

  for (let i = 0; i < 50; i++) {
    const level = levels[Math.floor(Math.random() * levels.length)];
    const source = sources[Math.floor(Math.random() * sources.length)];
    const message = messages[Math.floor(Math.random() * messages.length)];

    addLogEntry(level, source, message);
  }

  scrollToBottom();
}

function addMockLog() {
  const levels = ['debug', 'info', 'info', 'info', 'warn'];
  const sources = ['core', 'agent:coder', 'agent:researcher', 'skills:github'];
  const messages = [
    'Processing request',
    'Task completed',
    'Cache hit',
    'Skill executed',
    'Heartbeat sent',
  ];

  const level = levels[Math.floor(Math.random() * levels.length)];
  const source = sources[Math.floor(Math.random() * sources.length)];
  const message = messages[Math.floor(Math.random() * messages.length)];

  addLogEntry(level, source, message);
}

function addLogEntry(level, source, message) {
  const logsBody = document.getElementById('logsBody');
  if (!logsBody) return;

  const now = new Date();
  const timestamp = now.toTimeString().split(' ')[0];

  const entry = document.createElement('div');
  entry.className = 'log-line';
  /* nosemgrep: javascript.browser.security.insecure-document-method.insecure-document-method */
  entry.innerHTML = `
    <span class="timestamp">${timestamp}</span>
    <span class="level ${level}">${level.toUpperCase()}</span>
    <span class="source">[${source}]</span>
    <span class="message">${message}</span>
  `;

  logsBody.appendChild(entry);

  // Limit total logs
  while (logsBody.children.length > 200) {
    logsBody.removeChild(logsBody.firstChild);
  }

  // Update count
  const countEl = document.getElementById('logCount');
  if (countEl) {
    countEl.textContent = `${logsBody.children.length} entries`;
  }

  if (App.autoScroll) {
    scrollToBottom();
  }
}

function scrollToBottom() {
  const logsBody = document.getElementById('logsBody');
  if (logsBody) {
    logsBody.scrollTop = logsBody.scrollHeight;
  }
}

function toggleAutoScroll() {
  App.autoScroll = !App.autoScroll;
  Components.toast(`Auto-scroll ${App.autoScroll ? 'enabled' : 'disabled'}`, 'info');
}

function filterLogs(level) {
  const logsBody = document.getElementById('logsBody');
  if (!logsBody) return;

  const lines = logsBody.querySelectorAll('.log-line');
  lines.forEach((line) => {
    if (level === 'all' || line.querySelector('.level').classList.contains(level)) {
      line.style.display = '';
    } else {
      line.style.display = 'none';
    }
  });
}

function exportLogs() {
  const logsBody = document.getElementById('logsBody');
  if (!logsBody) return;

  const logs = Array.from(logsBody.querySelectorAll('.log-line'))
    .map((line) => {
      return line.textContent.trim().replace(/\s+/g, ' ');
    })
    .join('\n');

  Components.downloadAsFile(logs, `ag-claw-logs-${Date.now()}.txt`, 'text/plain');
  Components.toast('Logs exported', 'success');
}

function clearLogs() {
  const logsBody = document.getElementById('logsBody');
  if (logsBody) {
    logsBody.innerHTML = '';
    Components.toast('Logs cleared', 'info');
  }
}

// =============================================================================
// Memory
// =============================================================================

function filterMemories(type) {
  const memoryList = document.getElementById('memoryList');
  if (!memoryList) return;

  const items = memoryList.querySelectorAll('.memory-item');
  items.forEach((item) => {
    const badge = item.querySelector('.memory-item-title');
    if (badge) {
      const badgeType = badge.classList.contains('badge-info')
        ? 'semantic'
        : badge.classList.contains('badge-warning')
          ? 'episodic'
          : badge.classList.contains('badge-success')
            ? 'procedural'
            : 'all';
      item.style.display = type === 'all' || badgeType === type ? '' : 'none';
    }
  });
}

// =============================================================================
// Global Functions (exposed to window)
// =============================================================================

window.navigateTo = navigateTo;
window.refreshDashboard = refreshDashboard;
window.toggleTheme = toggleTheme;
window.showModal = showModal;
window.openAgentDetail = openAgentDetail;
window.closeAgentDetail = closeAgentDetail;
window.exportLogs = exportLogs;
window.clearLogs = clearLogs;
window.toggleAutoScroll = toggleAutoScroll;
window.filterLogs = filterLogs;
window.filterMemories = filterMemories;
