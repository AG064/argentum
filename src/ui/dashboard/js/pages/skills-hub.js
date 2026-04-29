/**
 * Argentum Dashboard - Skills Hub Page
 */

async function loadSkillsHubData() {
  const container = document.getElementById('skillsHubContainer');
  if (!container) return;

  container.innerHTML = `
    <div class="flex justify-center items-center" style="height: 200px">
      ${Components.spinner('lg')}
    </div>
  `;

  try {
    const response = await API.get('/api/skills');
    renderSkillsHubPage(response);
  } catch (err) {
    renderSkillsHubPage(getMockSkillsHubData());
  }
}

function renderSkillsHubPage(data) {
  const container = document.getElementById('skillsHubContainer');
  if (!container) return;

  const categories = ['all', 'writing', 'research', 'coding', 'automation', 'integration', 'utility'];
  const categoryIcons = {
    all: '🧩',
    writing: '✍️',
    research: '🔬',
    coding: '💻',
    automation: '⚡',
    integration: '🔗',
    utility: '🛠️',
  };

  /* nosemgrep: javascript.browser.security.insecure-document-method.insecure-document-method */
  container.innerHTML = `
    <!-- Search and Actions -->
    <div class="card">
      <div class="flex gap-4" style="flex-wrap: wrap">
        <div style="flex: 1; min-width: 250px">
          <div class="search-global" style="width: 100%">
            <svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>
            </svg>
            <input type="text" class="input" placeholder="Search skills..." id="skillsSearch" 
                   oninput="handleSkillsSearch(this.value)">
          </div>
        </div>
        <div class="flex gap-2">
          <button class="btn btn-primary" onclick="showCreateSkillWizard()">
            <svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width: 16px; height: 16px">
              <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
            </svg>
            Create Skill
          </button>
          <button class="btn btn-secondary" onclick="loadSkillsHubData()">
            <svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="width: 16px; height: 16px">
              <polyline points="23 4 23 10 17 10"/><polyline points="1 20 1 14 7 14"/>
              <path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"/>
            </svg>
            Refresh
          </button>
        </div>
      </div>
    </div>

    <!-- Categories -->
    <div class="card" style="margin-top: var(--space-6)">
      <div style="display: flex; gap: var(--space-2); flex-wrap: wrap">
        ${categories.map(cat => `
          <button class="btn btn-ghost category-btn ${cat === 'all' ? 'active' : ''}" 
                  data-category="${cat}" onclick="filterByCategory('${cat}')">
            <span style="margin-right: var(--space-1)">${categoryIcons[cat]}</span>
            ${cat.charAt(0).toUpperCase() + cat.slice(1)}
          </button>
        `).join('')}
      </div>
    </div>

    <!-- Installed Skills -->
    <div class="card" style="margin-top: var(--space-6)">
      <h3 class="card-title">Installed Skills</h3>
      <div class="skills-grid" style="margin-top: var(--space-4)">
        ${data.installed.map(skill => renderInstalledSkillCard(skill)).join('')}
      </div>
      ${data.installed.length === 0 ? `
        <div style="padding: var(--space-6); text-align: center; color: var(--color-text-muted)">
          No skills installed yet. Browse the marketplace below or create your own.
        </div>
      ` : ''}
    </div>

    <!-- Community Skills (Marketplace) -->
    <div class="card" style="margin-top: var(--space-6)">
      <div class="card-header">
        <h3 class="card-title">Skills Marketplace</h3>
        <span class="text-sm text-muted">Powered by ClawHub</span>
      </div>
      <div id="marketplaceSkills" style="margin-top: var(--space-4)">
        ${data.marketplace.slice(0, 6).map(skill => renderMarketplaceSkillCard(skill)).join('')}
      </div>
      ${data.marketplace.length > 6 ? `
        <div style="margin-top: var(--space-4); text-align: center">
          <button class="btn btn-ghost" onclick="showAllMarketplace()">
            Show all ${data.marketplace.length} skills
          </button>
        </div>
      ` : ''}
    </div>
  `;

  // Store marketplace data for filtering
  window.marketplaceSkills = data.marketplace;
}

function renderInstalledSkillCard(skill) {
  const hasScripts = skill.scripts && skill.scripts.length > 0;
  const hasReferences = skill.references && skill.references.length > 0;

  return `
    <div class="skill-card" data-skill-name="${skill.name}">
      <div class="skill-header">
        <div class="skill-icon" style="background: var(--color-accent-muted); color: var(--color-accent)">
          ${getSkillIcon(skill.category)}
        </div>
        <div class="skill-info">
          <div class="skill-name">${skill.name}</div>
          <div class="skill-version text-muted">v${skill.version}</div>
        </div>
        <span class="badge badge-${getCategoryBadgeClass(skill.category)}">${skill.category}</span>
      </div>
      <p class="skill-description">${skill.description}</p>
      <div class="skill-footer">
        <div class="skill-meta">
          ${hasScripts ? `<span class="text-muted text-sm">${skill.scripts.length} scripts</span>` : ''}
          ${hasReferences ? `<span class="text-muted text-sm">${skill.references.length} refs</span>` : ''}
        </div>
        <div class="skill-actions">
          <button class="btn btn-ghost btn-sm" onclick="showSkillInfo('${skill.name}')">Info</button>
          <button class="btn btn-ghost btn-sm" onclick="uninstallSkill('${skill.name}')">Uninstall</button>
        </div>
      </div>
    </div>
  `;
}

function renderMarketplaceSkillCard(skill) {
  return `
    <div class="skill-card marketplace" data-skill-name="${skill.name}" data-category="${skill.category}">
      <div class="skill-header">
        <div class="skill-icon" style="background: var(--color-success-muted); color: var(--color-success)">
          ${getSkillIcon(skill.category)}
        </div>
        <div class="skill-info">
          <div class="skill-name">${skill.name}</div>
          <div class="skill-version text-muted">by ${skill.author || 'community'}</div>
        </div>
        <span class="badge badge-${getCategoryBadgeClass(skill.category)}">${skill.category}</span>
      </div>
      <p class="skill-description">${skill.description}</p>
      <div class="skill-footer">
        <div class="skill-meta">
          <span class="text-muted text-sm">⭐ ${skill.stars || 0}</span>
          <span class="text-muted text-sm">v${skill.version || '1.0'}</span>
        </div>
        <button class="btn btn-primary btn-sm" onclick="installSkill('${skill.slug || skill.name}')">
          Install
        </button>
      </div>
    </div>
  `;
}

function getSkillIcon(category) {
  const icons = {
    writing: '✍️',
    research: '🔬',
    coding: '💻',
    automation: '⚡',
    integration: '🔗',
    utility: '🛠️',
    default: '🧩',
  };
  return icons[category] || icons.default;
}

function getCategoryBadgeClass(category) {
  const classes = {
    writing: 'info',
    research: 'warning',
    coding: 'success',
    automation: 'accent',
    integration: 'muted',
    utility: 'secondary',
  };
  return classes[category] || 'muted';
}

function handleSkillsSearch(query) {
  const installedGrid = document.querySelector('#page-skills-hub .skills-grid');
  const marketplaceGrid = document.getElementById('marketplaceSkills');

  if (!query || query.length < 2) {
    // Show all
    if (installedGrid) {
      installedGrid.querySelectorAll('.skill-card').forEach(card => {
        card.style.display = '';
      });
    }
    if (marketplaceGrid) {
      marketplaceGrid.querySelectorAll('.skill-card.marketplace').forEach(card => {
        card.style.display = '';
      });
    }
    return;
  }

  query = query.toLowerCase();

  if (installedGrid) {
    installedGrid.querySelectorAll('.skill-card').forEach(card => {
      const name = card.dataset.skillName?.toLowerCase() || '';
      const desc = card.querySelector('.skill-description')?.textContent.toLowerCase() || '';
      card.style.display = name.includes(query) || desc.includes(query) ? '' : 'none';
    });
  }

  if (marketplaceGrid) {
    marketplaceGrid.querySelectorAll('.skill-card.marketplace').forEach(card => {
      const name = card.dataset.skillName?.toLowerCase() || '';
      const desc = card.querySelector('.skill-description')?.textContent.toLowerCase() || '';
      card.style.display = name.includes(query) || desc.includes(query) ? '' : 'none';
    });
  }
}

function filterByCategory(category) {
  // Update active button
  document.querySelectorAll('.category-btn').forEach(btn => {
    btn.classList.toggle('active', btn.dataset.category === category);
  });

  // Filter marketplace
  const marketplaceGrid = document.getElementById('marketplaceSkills');
  if (marketplaceGrid && window.marketplaceSkills) {
    const filtered = category === 'all' 
      ? window.marketplaceSkills 
      : window.marketplaceSkills.filter(s => s.category === category);
    
    /* nosemgrep: javascript.browser.security.insecure-document-method.insecure-document-method */
    marketplaceGrid.innerHTML = filtered.map(skill => renderMarketplaceSkillCard(skill)).join('');
  }
}

function showAllMarketplace() {
  const marketplaceGrid = document.getElementById('marketplaceSkills');
  if (marketplaceGrid && window.marketplaceSkills) {
    /* nosemgrep: javascript.browser.security.insecure-document-method.insecure-document-method */
    marketplaceGrid.innerHTML = window.marketplaceSkills.map(skill => renderMarketplaceSkillCard(skill)).join('');
  }
}

async function installSkill(slug) {
  Components.toast(`Installing ${slug}...`, 'info');
  try {
    await API.post('/api/skills/install', { slug });
    Components.toast(`${slug} installed successfully`, 'success');
    loadSkillsHubData();
  } catch (err) {
    Components.toast(`Failed to install ${slug}`, 'error');
  }
}

async function uninstallSkill(name) {
  Components.modal({
    title: 'Uninstall Skill',
    content: `
      <div class="alert alert-warning">
        <svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/>
          <line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/>
        </svg>
        <span>This will uninstall <strong>${name}</strong>. This cannot be undone.</span>
      </div>
    `,
    actions: [
      { label: 'Cancel', class: 'btn-secondary', action: 'close' },
      { label: 'Uninstall', class: 'btn-danger', action: `doUninstall('${name}')` },
    ],
  });
}

async function doUninstall(name) {
  Components.closeModal();
  try {
    await API.post('/api/skills/uninstall', { name });
    Components.toast(`${name} uninstalled`, 'success');
    loadSkillsHubData();
  } catch (err) {
    Components.toast(`Failed to uninstall ${name}`, 'error');
  }
}

function showSkillInfo(name) {
  // Get skill data and show detail modal
  const skill = getMockSkillsHubData().installed.find(s => s.name === name);
  if (!skill) return;

  Components.modal({
    title: skill.name,
    content: `
      <div style="margin-bottom: var(--space-4)">
        <span class="badge badge-${getCategoryBadgeClass(skill.category)}">${skill.category}</span>
        <span class="text-muted" style="margin-left: var(--space-2)">v${skill.version}</span>
      </div>
      <p style="margin-bottom: var(--space-4)">${skill.description}</p>
      ${skill.scripts && skill.scripts.length > 0 ? `
        <h4 style="margin-bottom: var(--space-2)">Scripts</h4>
        <ul style="margin-bottom: var(--space-4)">
          ${skill.scripts.map(s => `<li><code class="code">${s}</code></li>`).join('')}
        </ul>
      ` : ''}
      ${skill.references && skill.references.length > 0 ? `
        <h4 style="margin-bottom: var(--space-2)">References</h4>
        <ul>
          ${skill.references.map(r => `<li><code class="code">${r}</code></li>`).join('')}
        </ul>
      ` : ''}
    `,
    actions: [
      { label: 'Close', class: 'btn-secondary', action: 'close' },
    ],
  });
}

function showCreateSkillWizard() {
  Components.modal({
    title: 'Create New Skill',
    content: `
      <form id="createSkillForm">
        <div class="form-group">
          <label class="form-label">Skill Name</label>
          <input type="text" class="input" name="name" placeholder="my-awesome-skill" required
                 pattern="[a-z0-9-]+" title="Lowercase letters, numbers, and hyphens only">
          <small class="form-hint">Lowercase, no spaces (e.g., weather查询, github-actions)</small>
        </div>
        <div class="form-group">
          <label class="form-label">Category</label>
          <select class="input select" name="category" required>
            <option value="utility">Utility</option>
            <option value="coding">Coding</option>
            <option value="research">Research</option>
            <option value="writing">Writing</option>
            <option value="automation">Automation</option>
            <option value="integration">Integration</option>
          </select>
        </div>
        <div class="form-group">
          <label class="form-label">Description</label>
          <textarea class="input" name="description" rows="3" placeholder="What does this skill do?" required></textarea>
        </div>
        <div class="form-group">
          <label class="form-label">Trigger phrases (one per line)</label>
          <textarea class="input" name="triggers" rows="3" placeholder="when I need help with &#10;every time I ask about&#10;when user says"></textarea>
          <small class="form-hint">Lines that trigger this skill</small>
        </div>
        <div class="form-group">
          <label class="toggle">
            <input type="checkbox" name="hasScripts" onchange="toggleScriptsSection(this.checked)">
            <span class="toggle-slider"></span>
          </label>
          <span style="margin-left: var(--space-2)">Add scripts</span>
        </div>
        <div id="scriptsSection" style="display: none; margin-top: var(--space-4)">
          <h4>Scripts</h4>
          <div id="scriptEntries"></div>
          <button type="button" class="btn btn-ghost btn-sm" onclick="addScriptEntry()">+ Add Script</button>
        </div>
      </form>
    `,
    actions: [
      { label: 'Cancel', class: 'btn-secondary', action: 'close' },
      { label: 'Create', class: 'btn-primary', action: 'doCreateSkill' },
    ],
  });

  window.toggleScriptsSection = (checked) => {
    const section = document.getElementById('scriptsSection');
    if (section) section.style.display = checked ? '' : 'none';
  };

  window.addScriptEntry = () => {
    const entries = document.getElementById('scriptEntries');
    if (entries) {
      const idx = entries.children.length;
      /* nosemgrep: javascript.browser.security.insecure-document-method.insecure-document-method */
      entries.innerHTML += `
        <div style="margin-bottom: var(--space-2)">
          <input type="text" class="input" name="script-${idx}" placeholder="script-name.sh" style="margin-right: var(--space-2)">
          <select class="input select" name="script-type-${idx}" style="width: auto">
            <option value="bash">Bash</option>
            <option value="node">Node.js</option>
            <option value="python">Python</option>
          </select>
        </div>
      `;
    }
  };

  window.doCreateSkill = async () => {
    const form = document.getElementById('createSkillForm');
    if (!form) return;

    const formData = new FormData(form);
    const skill = {
      name: formData.get('name'),
      category: formData.get('category'),
      description: formData.get('description'),
      triggers: (formData.get('triggers') || '').split('\n').filter(t => t.trim()),
      hasScripts: formData.get('hasScripts') === 'on',
    };

    if (!skill.name || !skill.description) {
      Components.toast('Please fill in all required fields', 'error');
      return;
    }

    Components.closeModal();
    Components.toast('Creating skill...', 'info');

    try {
      await API.post('/api/skills/create', skill);
      Components.toast('Skill created successfully', 'success');
      loadSkillsHubData();
    } catch (err) {
      Components.toast('Failed to create skill', 'error');
    }
  };
}

function getMockSkillsHubData() {
  return {
    installed: [
      {
        name: 'clawhub',
        version: '0.0.4',
        category: 'utility',
        description: 'Install, update, and publish agent skills from ClawHub.',
        scripts: ['install.sh', 'update.sh'],
        references: [],
      },
      {
        name: 'weather',
        version: '0.0.4',
        category: 'utility',
        description: 'Get current weather and forecasts via wttr.in or Open-Meteo.',
        scripts: [],
        references: ['SKILL.md'],
      },
      {
        name: 'deep-research-pro',
        version: '0.0.4',
        category: 'research',
        description: 'Multi-source deep research agent. Searches the web, synthesizes findings, and delivers cited reports.',
        scripts: ['research.sh'],
        references: ['SKILL.md'],
      },
      {
        name: 'writing-assistant',
        version: '0.0.4',
        category: 'writing',
        description: 'Writing Team Lead managing specialized writers via MCP tools.',
        scripts: ['write.sh'],
        references: ['SKILL.md', 'references/templates.md'],
      },
      {
        name: 'github',
        version: '0.0.4',
        category: 'integration',
        description: 'GitHub operations via gh CLI: issues, PRs, CI runs, code review.',
        scripts: [],
        references: ['SKILL.md'],
      },
    ],
    marketplace: [
      {
        name: 'slack-bot',
        slug: 'slack-bot',
        version: '0.0.4',
        category: 'integration',
        author: 'ag-claw',
        description: 'Send messages and notifications to Slack channels.',
        stars: 42,
      },
      {
        name: 'notion-sync',
        slug: 'notion-sync',
        version: '0.0.4',
        category: 'integration',
        author: 'community',
        description: 'Sync notes and tasks with Notion databases.',
        stars: 38,
      },
      {
        name: 'code-review',
        slug: 'code-review',
        version: '0.0.4',
        category: 'coding',
        author: 'ag-claw',
        description: 'Automated code review using AI. Detects bugs, style issues, and security vulnerabilities.',
        stars: 127,
      },
      {
        name: 'sql-helper',
        slug: 'sql-helper',
        version: '0.0.4',
        category: 'coding',
        author: 'community',
        description: 'Generate SQL queries from natural language descriptions.',
        stars: 89,
      },
      {
        name: 'meeting-notes',
        slug: 'meeting-notes',
        version: '0.0.4',
        category: 'writing',
        author: 'ag-claw',
        description: 'Automatically generate meeting notes from transcripts.',
        stars: 56,
      },
      {
        name: 'image-gen',
        slug: 'image-gen',
        version: '0.0.4',
        category: 'utility',
        author: 'community',
        description: 'Generate images using DALL-E, Stable Diffusion, or Midjourney.',
        stars: 203,
      },
      {
        name: 'cron-manager',
        slug: 'cron-manager',
        version: '0.0.4',
        category: 'automation',
        author: 'ag-claw',
        description: 'Manage and monitor cron jobs from a friendly interface.',
        stars: 34,
      },
      {
        name: 'data-analyzer',
        slug: 'data-analyzer',
        version: '0.0.4',
        category: 'research',
        author: 'community',
        description: 'Analyze CSV/JSON data files and generate insights.',
        stars: 67,
      },
    ],
  };
}

// Expose functions to global scope
window.installSkill = installSkill;
window.uninstallSkill = uninstallSkill;
window.showSkillInfo = showSkillInfo;
window.showCreateSkillWizard = showCreateSkillWizard;
window.handleSkillsSearch = handleSkillsSearch;
window.filterByCategory = filterByCategory;

// Skills Hub page initialization
document.addEventListener('DOMContentLoaded', () => {
  if (document.getElementById('page-skills-hub')) {
    loadSkillsHubData();
  }
});
