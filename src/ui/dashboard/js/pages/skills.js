/**
 * AG-Claw Dashboard - Skills Page
 */

// Mock skills data
const mockSkills = [
  {
    id: 'github',
    name: 'GitHub Integration',
    author: 'AG-Claw',
    description:
      'Seamless GitHub integration for repository management, issues, pull requests, and workflow automation.',
    icon: '🐙',
    rating: 4.8,
    reviews: 234,
    installs: 12453,
    category: 'communication',
    installed: true,
  },
  {
    id: 'deep-research-pro',
    name: 'Deep Research Pro',
    author: 'AG-Claw',
    description:
      'Multi-source deep research agent. Searches the web, synthesizes findings, and delivers cited reports.',
    icon: '🔍',
    rating: 4.9,
    reviews: 189,
    installs: 8932,
    category: 'memory',
    installed: true,
  },
  {
    id: 'telegram',
    name: 'Telegram Bot',
    author: 'AG-Claw',
    description:
      'Design Telegram Bot API workflows and command-driven conversations using direct HTTPS requests.',
    icon: '✈️',
    rating: 4.7,
    reviews: 312,
    installs: 15621,
    category: 'communication',
    installed: true,
  },
  {
    id: 'weather',
    name: 'Weather Forecast',
    author: 'Community',
    description: 'Get current weather and forecasts via wttr.in or Open-Meteo. No API key needed.',
    icon: '🌤️',
    rating: 4.5,
    reviews: 87,
    installs: 4521,
    category: 'automation',
    installed: false,
  },
  {
    id: 'himalaya',
    name: 'Email Management',
    author: 'Community',
    description:
      'CLI to manage emails via IMAP/SMTP. Supports multiple accounts and message composition.',
    icon: '📧',
    rating: 4.6,
    reviews: 156,
    installs: 7823,
    category: 'automation',
    installed: false,
  },
  {
    id: 'voice',
    name: 'Voice Assistant',
    author: 'AG-Claw',
    description: 'Text-to-speech and speech-to-text capabilities with ElevenLabs integration.',
    icon: '🎙️',
    rating: 4.4,
    reviews: 98,
    installs: 3241,
    category: 'automation',
    installed: false,
  },
  {
    id: 'calendar',
    name: 'Calendar Integration',
    author: 'AG-Claw',
    description: 'Google Calendar integration for scheduling, reminders, and event management.',
    icon: '📅',
    rating: 4.3,
    reviews: 67,
    installs: 2156,
    category: 'automation',
    installed: false,
  },
  {
    id: 'healthcheck',
    name: 'Health Monitor',
    author: 'AG-Claw',
    description:
      'Host security hardening and risk-tolerance configuration for OpenClaw deployments.',
    icon: '🛡️',
    rating: 4.8,
    reviews: 145,
    installs: 6789,
    category: 'security',
    installed: true,
  },
];

function loadSkills() {
  const grid = document.getElementById('skillsGrid');
  if (!grid) return;

  grid.innerHTML = mockSkills
    .map(
      (skill) => `
    <div class="skill-card ${skill.installed ? 'installed' : ''}">
      <div class="skill-header">
        <div class="skill-icon">${skill.icon}</div>
        <div class="skill-info">
          <div class="skill-name">${skill.name}</div>
          <div class="skill-author">by ${skill.author}</div>
        </div>
        <div class="skill-rating">
          ${Components.starRating(skill.rating)}
          <span class="rating-count">(${Components.formatNumber(skill.reviews)})</span>
        </div>
      </div>
      
      <div class="skill-description">${skill.description}</div>
      
      <div class="skill-footer">
        <div class="skill-meta">
          <span>${Components.formatNumber(skill.installs)} installs</span>
          <span class="category-badge">
            <span class="dot" style="background: var(--color-cat-${skill.category})"></span>
            ${skill.category}
          </span>
        </div>
        <button class="skill-install-btn ${skill.installed ? 'installed' : 'install'}" 
                onclick="toggleSkill('${skill.id}')"
                ${skill.installed ? 'disabled' : ''}>
          ${skill.installed ? '✓ Installed' : 'Install'}
        </button>
      </div>
    </div>
  `,
    )
    .join('');

  // Setup search
  const searchInput = document.getElementById('skillSearch');
  if (searchInput) {
    searchInput.addEventListener(
      'input',
      Components.debounce((e) => {
        filterSkills(e.target.value);
      }, 300),
    );
  }
}

function filterSkills(query) {
  const grid = document.getElementById('skillsGrid');
  if (!grid) return;

  const cards = grid.querySelectorAll('.skill-card');
  const lowerQuery = query.toLowerCase();

  cards.forEach((card) => {
    const name = card.querySelector('.skill-name')?.textContent.toLowerCase() || '';
    const description = card.querySelector('.skill-description')?.textContent.toLowerCase() || '';
    const author = card.querySelector('.skill-author')?.textContent.toLowerCase() || '';

    if (
      name.includes(lowerQuery) ||
      description.includes(lowerQuery) ||
      author.includes(lowerQuery)
    ) {
      card.style.display = '';
    } else {
      card.style.display = 'none';
    }
  });
}

async function toggleSkill(skillId) {
  const skill = mockSkills.find((s) => s.id === skillId);
  if (!skill) return;

  if (skill.installed) {
    // Confirm uninstall
    const confirmed = await Components.confirm({
      title: 'Uninstall Skill',
      message: `Are you sure you want to uninstall "${skill.name}"? This action cannot be undone.`,
      confirmText: 'Uninstall',
      danger: true,
    });

    if (confirmed) {
      Components.toast(`Uninstalling ${skill.name}...`, 'info');
      setTimeout(() => {
        skill.installed = false;
        loadSkills();
        Components.toast(`${skill.name} uninstalled`, 'success');
      }, 1000);
    }
  } else {
    Components.toast(`Installing ${skill.name}...`, 'info');
    setTimeout(() => {
      skill.installed = true;
      loadSkills();
      Components.toast(`${skill.name} installed successfully`, 'success');
    }, 1500);
  }
}

// Initialize
document.addEventListener('DOMContentLoaded', () => {
  // Skills will be loaded when page is navigated to
});
