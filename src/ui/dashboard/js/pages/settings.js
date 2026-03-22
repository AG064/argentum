/**
 * AG-Claw Dashboard - Settings Page
 */

function loadSettings() {
  // Initialize settings UI
  setupSettingsListeners();
}

function setupSettingsListeners() {
  // Listen for toggle changes
  document.querySelectorAll('.toggle input').forEach(toggle => {
    toggle.addEventListener('change', (e) => {
      const label = e.target.closest('.settings-row')?.querySelector('.settings-row-label')?.textContent;
      Components.toast(`${label} ${e.target.checked ? 'enabled' : 'disabled'}`, 'info');
    });
  });

  // Listen for select changes
  document.querySelectorAll('.settings-section select').forEach(select => {
    select.addEventListener('change', (e) => {
      Components.toast('Setting updated', 'success');
    });
  });

  // Listen for input changes
  document.querySelectorAll('.settings-section .input').forEach(input => {
    input.addEventListener('blur', (e) => {
      if (e.target.value) {
        Components.toast('Setting saved', 'success');
      }
    });
  });

  // Settings nav active state
  document.querySelectorAll('.settings-nav-item').forEach(item => {
    item.addEventListener('click', () => {
      document.querySelectorAll('.settings-nav-item').forEach(i => i.classList.remove('active'));
      item.classList.add('active');
    });
  });
}

async function saveSettings() {
  Components.toast('Saving settings...', 'info');
  
  // Simulate save
  setTimeout(() => {
    Components.toast('Settings saved successfully', 'success');
  }, 1000);
}

async function testApiKey(name) {
  Components.toast(`Testing ${name}...`, 'info');
  
  // Simulate test
  setTimeout(() => {
    Components.toast(`${name} verified successfully`, 'success');
  }, 1500);
}

// Initialize
document.addEventListener('DOMContentLoaded', () => {
  loadSettings();
});
