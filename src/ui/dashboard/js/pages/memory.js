/**
 * AG-Claw Dashboard - Memory Page
 */

// Mock memory data
const mockMemories = [
  {
    id: 'mem_a7x9k2',
    type: 'semantic',
    title: 'User Preferences',
    content:
      'User preferences updated: Preferred language set to English, timezone changed to UTC, notification preferences modified for email and Telegram channels.',
    timestamp: Date.now() - 2 * 60 * 60 * 1000,
  },
  {
    id: 'mem_b3n8m4',
    type: 'episodic',
    title: 'Session Started',
    content:
      'Session started with user AG. Initial context loaded from MEMORY.md. User requested dashboard overview and agent status check. All systems nominal.',
    timestamp: Date.now() - 5 * 60 * 60 * 1000,
  },
  {
    id: 'mem_c5p2q7',
    type: 'procedural',
    title: 'Backup Completed',
    content:
      'Automated backup procedure executed successfully. Backed up 847 files totaling 2.3 GB to /backups/2024-03-22. Compression ratio: 68%.',
    timestamp: Date.now() - 24 * 60 * 60 * 1000,
  },
  {
    id: 'mem_d9r4s1',
    type: 'semantic',
    title: 'User Profile',
    content:
      'Learned user interest in AI, space exploration, programming, and hardware. Photography and 3D modeling noted as creative hobbies. Game design experience from 5CA support role.',
    timestamp: Date.now() - 48 * 60 * 60 * 1000,
  },
];

function loadMemories() {
  const list = document.getElementById('memoryList');
  if (!list) return;

  renderMemories(mockMemories);

  // Setup search
  const searchInput = document.getElementById('memorySearch');
  if (searchInput) {
    searchInput.addEventListener(
      'input',
      Components.debounce((e) => {
        searchMemories(e.target.value);
      }, 300),
    );
  }
}

function renderMemories(memories) {
  const list = document.getElementById('memoryList');
  if (!list) return;

  /* nosemgrep: javascript.browser.security.insecure-document-method.insecure-document-method */
  list.innerHTML = memories
    .map((memory) => {
      const typeBadge = getTypeBadge(memory.type);
      return `
      <div class="memory-item">
        <div class="memory-item-header">
          <span class="memory-item-title ${typeBadge}">${memory.type.charAt(0).toUpperCase() + memory.type.slice(1)}</span>
          <span class="memory-item-time">${Components.formatRelativeTime(memory.timestamp)}</span>
        </div>
        <div class="memory-item-preview">${memory.content}</div>
        <div class="memory-item-footer">
          <span class="text-xs text-tertiary">ID: ${memory.id}</span>
          <div class="flex gap-2">
            <button class="btn btn-ghost btn-sm" onclick="editMemory('${memory.id}')">Edit</button>
            <button class="btn btn-ghost btn-sm" onclick="deleteMemory('${memory.id}')">Delete</button>
          </div>
        </div>
      </div>
    `;
    })
    .join('');
}

function getTypeBadge(type) {
  switch (type) {
    case 'semantic':
      return 'badge-info';
    case 'episodic':
      return 'badge-warning';
    case 'procedural':
      return 'badge-success';
    default:
      return 'badge-default';
  }
}

function searchMemories(query) {
  if (!query.trim()) {
    renderMemories(mockMemories);
    return;
  }

  const lowerQuery = query.toLowerCase();
  const filtered = mockMemories.filter(
    (memory) =>
      memory.title.toLowerCase().includes(lowerQuery) ||
      memory.content.toLowerCase().includes(lowerQuery) ||
      memory.type.toLowerCase().includes(lowerQuery),
  );

  renderMemories(filtered);
}

async function editMemory(id) {
  const memory = mockMemories.find((m) => m.id === id);
  if (!memory) return;

  Components.alert('Edit Memory', `Edit form for "${memory.title}" would appear here.`);
}

async function deleteMemory(id) {
  const memory = mockMemories.find((m) => m.id === id);
  if (!memory) return;

  const confirmed = await Components.confirm({
    title: 'Delete Memory',
    message: `Are you sure you want to delete "${memory.title}"? This action cannot be undone.`,
    confirmText: 'Delete',
    danger: true,
  });

  if (confirmed) {
    const index = mockMemories.findIndex((m) => m.id === id);
    if (index > -1) {
      mockMemories.splice(index, 1);
      loadMemories();
      Components.toast('Memory deleted', 'success');
    }
  }
}

// Initialize
document.addEventListener('DOMContentLoaded', () => {
  // Memories will be loaded when page is navigated to
});
