/**
 * AG-Claw Dashboard - UI Components
 * Reusable component builders
 */

const Components = {
  /**
   * Create a toast notification
   */
  toast(message, type = 'info', duration = 5000) {
    const container = document.getElementById('toastContainer');
    if (!container) return;

    const icons = {
      success:
        '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><polyline points="20 6 9 17 4 12"/></svg>',
      error:
        '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="15" y1="9" x2="9" y2="15"/><line x1="9" y1="9" x2="15" y2="15"/></svg>',
      warning:
        '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/><line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>',
      info: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="10"/><line x1="12" y1="16" x2="12" y2="12"/><line x1="12" y1="8" x2="12.01" y2="8"/></svg>',
    };

    const toast = document.createElement('div');
    toast.className = `toast ${type}`;
    toast.innerHTML = `
      <div class="toast-icon">${icons[type]}</div>
      <div class="toast-content">
        <div class="toast-message">${message}</div>
      </div>
      <button class="toast-close" onclick="this.parentElement.remove()">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
        </svg>
      </button>
    `;

    container.appendChild(toast);

    // Auto remove
    if (duration > 0) {
      setTimeout(() => {
        toast.style.opacity = '0';
        toast.style.transform = 'translateX(100%)';
        setTimeout(() => toast.remove(), 300);
      }, duration);
    }

    return toast;
  },

  /**
   * Create a confirmation modal
   */
  confirm(options) {
    return new Promise((resolve) => {
      const {
        title,
        message,
        confirmText = 'Confirm',
        cancelText = 'Cancel',
        danger = false,
      } = options;

      const backdrop = document.createElement('div');
      backdrop.className = 'modal-backdrop open';
      backdrop.innerHTML = `
        <div class="modal" style="max-width: 400px;">
          <div class="modal-header">
            <h3 class="modal-title">${title}</h3>
            <button class="modal-close" data-action="cancel">×</button>
          </div>
          <div class="modal-body">
            <p style="color: var(--color-text-secondary);">${message}</p>
          </div>
          <div class="modal-footer">
            <button class="btn btn-secondary" data-action="cancel">${cancelText}</button>
            <button class="btn ${danger ? 'btn-danger' : 'btn-primary'}" data-action="confirm">${confirmText}</button>
          </div>
        </div>
      `;

      document.body.appendChild(backdrop);

      const close = (result) => {
        backdrop.classList.remove('open');
        setTimeout(() => backdrop.remove(), 200);
        resolve(result);
      };

      backdrop.addEventListener('click', (e) => {
        const action = e.target.dataset?.action;
        if (action === 'confirm') close(true);
        if (action === 'cancel' || e.target === backdrop) close(false);
      });
    });
  },

  /**
   * Create an alert modal
   */
  alert(title, message, type = 'info') {
    const icons = {
      success:
        '<svg class="toast-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="color: var(--color-success)"><polyline points="20 6 9 17 4 12"/></svg>',
      error:
        '<svg class="toast-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="color: var(--color-error)"><circle cx="12" cy="12" r="10"/><line x1="15" y1="9" x2="9" y2="15"/><line x1="9" y1="9" x2="15" y2="15"/></svg>',
      warning:
        '<svg class="toast-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="color: var(--color-warning)"><path d="M10.29 3.86L1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0z"/><line x1="12" y1="9" x2="12" y2="13"/><line x1="12" y1="17" x2="12.01" y2="17"/></svg>',
      info: '<svg class="toast-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" style="color: var(--color-info)"><circle cx="12" cy="12" r="10"/><line x1="12" y1="16" x2="12" y2="12"/><line x1="12" y1="8" x2="12.01" y2="8"/></svg>',
    };

    return new Promise((resolve) => {
      const backdrop = document.createElement('div');
      backdrop.className = 'modal-backdrop open';
      backdrop.innerHTML = `
        <div class="modal" style="max-width: 400px;">
          <div class="modal-header">
            <h3 class="modal-title">${title}</h3>
            <button class="modal-close" data-action="close">×</button>
          </div>
          <div class="modal-body">
            <div class="flex gap-3" style="align-items: flex-start;">
              ${icons[type]}
              <p style="color: var(--color-text-secondary);">${message}</p>
            </div>
          </div>
          <div class="modal-footer">
            <button class="btn btn-primary" data-action="close">OK</button>
          </div>
        </div>
      `;

      document.body.appendChild(backdrop);

      backdrop.addEventListener('click', (e) => {
        if (e.target.dataset?.action === 'close' || e.target === backdrop) {
          backdrop.classList.remove('open');
          setTimeout(() => backdrop.remove(), 200);
          resolve();
        }
      });
    });
  },

  /**
   * Create a dropdown menu
   */
  dropdown(trigger, items) {
    const dropdown = document.createElement('div');
    dropdown.className = 'dropdown';

    dropdown.innerHTML = `<div class="dropdown-menu">${items
      .map((item) => {
        if (item.divider) {
          return '<div class="dropdown-divider"></div>';
        }
        return `<button class="dropdown-item ${item.danger ? 'danger' : ''}" data-action="${item.action}">${item.label}</button>`;
      })
      .join('')}</div>`;

    trigger.appendChild(dropdown);

    // Toggle on click
    trigger.addEventListener('click', (e) => {
      e.stopPropagation();
      document.querySelectorAll('.dropdown.open').forEach((d) => {
        if (d !== dropdown) d.classList.remove('open');
      });
      dropdown.classList.toggle('open');
    });

    // Handle item clicks
    dropdown.addEventListener('click', (e) => {
      const action = e.target.dataset?.action;
      if (action) {
        const item = items.find((i) => i.action === action);
        if (item?.onClick) {
          item.onClick();
        }
        dropdown.classList.remove('open');
      }
    });

    // Close on outside click
    document.addEventListener('click', () => {
      dropdown.classList.remove('open');
    });

    return dropdown;
  },

  /**
   * Format bytes to human readable
   */
  formatBytes(bytes) {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
  },

  /**
   * Format duration to human readable
   */
  formatDuration(ms) {
    const seconds = Math.floor(ms / 1000);
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);
    const days = Math.floor(hours / 24);

    if (days > 0) return `${days}d ${hours % 24}h`;
    if (hours > 0) return `${hours}h ${minutes % 60}m`;
    if (minutes > 0) return `${minutes}m ${seconds % 60}s`;
    return `${seconds}s`;
  },

  /**
   * Format relative time
   */
  formatRelativeTime(date) {
    const now = new Date();
    const diff = now - new Date(date);
    const seconds = Math.floor(diff / 1000);
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);
    const days = Math.floor(hours / 24);

    if (seconds < 60) return 'just now';
    if (minutes < 60) return `${minutes} minute${minutes > 1 ? 's' : ''} ago`;
    if (hours < 24) return `${hours} hour${hours > 1 ? 's' : ''} ago`;
    if (days < 7) return `${days} day${days > 1 ? 's' : ''} ago`;
    return new Date(date).toLocaleDateString();
  },

  /**
   * Format number with commas
   */
  formatNumber(num) {
    return num.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ',');
  },

  /**
   * Truncate text
   */
  truncate(text, length = 100) {
    if (text.length <= length) return text;
    return text.substring(0, length).trim() + '...';
  },

  /**
   * Debounce function
   */
  debounce(func, wait) {
    let timeout;
    return function executedFunction(...args) {
      const later = () => {
        clearTimeout(timeout);
        func(...args);
      };
      clearTimeout(timeout);
      timeout = setTimeout(later, wait);
    };
  },

  /**
   * Create loading spinner
   */
  spinner(size = 'md') {
    const sizes = { sm: 14, md: 20, lg: 28, xl: 40 };
    const dim = sizes[size] || sizes.md;
    return `<div class="spinner spinner-${size}" style="width: ${dim}px; height: ${dim}px;"></div>`;
  },

  /**
   * Create skeleton loader
   */
  skeleton(width = '100%', height = '20px') {
    return `<div class="skeleton" style="width: ${width}; height: ${height};"></div>`;
  },

  /**
   * Create empty state
   */
  emptyState(icon, title, description, action = null) {
    return `
      <div class="empty-state">
        <div class="empty-state-icon">${icon}</div>
        <h3 class="empty-state-title">${title}</h3>
        <p class="empty-state-description">${description}</p>
        ${action ? `<button class="btn btn-primary" onclick="${action.onClick}">${action.label}</button>` : ''}
      </div>
    `;
  },

  /**
   * Create badge
   */
  badge(text, type = 'default') {
    const types = {
      default: 'badge-default',
      primary: 'badge-primary',
      success: 'badge-success',
      warning: 'badge-warning',
      error: 'badge-error',
      info: 'badge-info',
    };
    return `<span class="badge ${types[type]}">${text}</span>`;
  },

  /**
   * Create status dot
   */
  statusDot(status) {
    return `<span class="status-dot ${status}"></span>`;
  },

  /**
   * Create star rating
   */
  starRating(rating, max = 5) {
    let html = '<div class="stars">';
    for (let i = 1; i <= max; i++) {
      if (i <= rating) {
        html +=
          '<svg class="star" viewBox="0 0 24 24" fill="currentColor"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/></svg>';
      } else if (i - 0.5 <= rating) {
        html +=
          '<svg class="star" viewBox="0 0 24 24" fill="currentColor" opacity="0.5"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/></svg>';
      } else {
        html +=
          '<svg class="star" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" opacity="0.3"><polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/></svg>';
      }
    }
    html += '</div>';
    return html;
  },

  /**
   * Create progress bar
   */
  progressBar(value, max = 100, type = '') {
    const percent = Math.min(100, Math.max(0, (value / max) * 100));
    return `
      <div class="progress">
        <div class="progress-bar ${type}" style="width: ${percent}%"></div>
      </div>
    `;
  },

  /**
   * Create code block
   */
  codeBlock(code, language = '') {
    return `
      <div class="code-block">
        <div class="code-header">
          <span class="code-title">${language || 'code'}</span>
          <button class="code-copy" onclick="navigator.clipboard.writeText(\`${code.replace(/`/g, '\\`')}\`)">Copy</button>
        </div>
        <pre class="code-content">${code}</pre>
      </div>
    `;
  },

  /**
   * Copy text to clipboard
   */
  async copyToClipboard(text) {
    try {
      await navigator.clipboard.writeText(text);
      this.toast('Copied to clipboard', 'success');
      return true;
    } catch (err) {
      console.error('Failed to copy:', err);
      return false;
    }
  },

  /**
   * Download text as file
   */
  downloadAsFile(content, filename, type = 'text/plain') {
    const blob = new Blob([content], { type });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  },
};

// Make Components globally available
window.Components = Components;
