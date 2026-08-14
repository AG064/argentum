const paths = {
  'agent':
    '<path d="M12 3l7 4v6c0 4-3 7-7 8-4-1-7-4-7-8V7l7-4z"/><path d="M9 12h6"/><path d="M10 16h4"/>',
  'bell': '<path d="M18 8a6 6 0 1 0-12 0c0 7-3 7-3 9h18c0-2-3-2-3-9"/><path d="M10 21h4"/>',
  'blocks':
    '<rect x="4" y="4" width="7" height="7" rx="1"/><rect x="13" y="4" width="7" height="7" rx="1"/><rect x="4" y="13" width="7" height="7" rx="1"/><rect x="13" y="13" width="7" height="7" rx="1"/>',
  'channel':
    '<path d="M5 12h4"/><path d="M15 12h4"/><circle cx="12" cy="12" r="3"/><circle cx="4" cy="12" r="2"/><circle cx="20" cy="12" r="2"/>',
  'chat': '<path d="M5 5h14v10H8l-3 3V5z"/><path d="M8 9h8"/><path d="M8 12h5"/>',
  'externalLink':
    '<path d="M14 4h6v6"/><path d="M20 4l-9 9"/><path d="M20 14v4a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h4"/>',
  'plus': '<path d="M12 5v14"/><path d="M5 12h14"/>',
  'x': '<path d="M18 6L6 18"/><path d="M6 6l12 12"/>',
  'trash':
    '<path d="M4 7h16"/><path d="M10 11v6"/><path d="M14 11v6"/><path d="M6 7l1 14h10l1-14"/><path d="M9 7V4h6v3"/>',
  'send': '<path d="M22 2L11 13"/><path d="M22 2l-7 20-4-9-9-4 20-7z"/>',
  'transmit':
    '<circle cx="4" cy="12" r="1.5"/><path d="M6 12h11"/><path d="M13 7l5 5-5 5"/><path d="M7.5 7.5l2 1.5"/><path d="M7.5 16.5l2-1.5"/>',
  'stop': '<rect x="7" y="7" width="10" height="10" rx="1"/>',
  'copy':
    '<rect x="8" y="8" width="11" height="11" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h8a2 2 0 0 1 2 2v1"/>',
  'cpu':
    '<rect x="5" y="5" width="14" height="14" rx="2"/><path d="M9 9h6v6H9z"/><path d="M9 2v3"/><path d="M15 2v3"/><path d="M9 19v3"/><path d="M15 19v3"/><path d="M2 9h3"/><path d="M2 15h3"/><path d="M19 9h3"/><path d="M19 15h3"/>',
  'image':
    '<rect x="3" y="5" width="18" height="14" rx="2"/><circle cx="8" cy="10" r="2"/><path d="M21 15l-5-5L5 19"/>',
  'file':
    '<path d="M14 3H6a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V9z"/><path d="M14 3v6h6"/>',
  'folder': '<path d="M3 6h7l2 2h9v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6z"/><path d="M3 10h18"/>',
  'refresh':
    '<path d="M21 12a9 9 0 0 1-15.5 6.2L3 16"/><path d="M3 21v-5h5"/><path d="M3 12A9 9 0 0 1 18.5 5.8L21 8"/><path d="M21 3v5h-5"/>',
  'retry': '<path d="M5 12a7 7 0 1 0 2-5"/><path d="M5 5v7h7"/><path d="M11 9l5 3-5 3z"/>',
  'orbit':
    '<circle cx="12" cy="12" r="2"/><ellipse cx="12" cy="12" rx="9" ry="4" transform="rotate(-24 12 12)"/><path d="M19 6h.01"/>',
  'arrowLeft': '<path d="M19 12H5"/><path d="M11 18l-6-6 6-6"/>',
  'chevronDown': '<path d="M6 9l6 6 6-6"/>',
  'chevronUp': '<path d="M6 15l6-6 6 6"/>',
  'alert':
    '<path d="M12 9v4"/><path d="M12 17h.01"/><path d="M10.3 3.9L2.5 18a2 2 0 0 0 1.8 3h15.4a2 2 0 0 0 1.8-3L13.7 3.9a2 2 0 0 0-3.4 0z"/>',
  'check': '<path d="M20 6L9 17l-5-5"/>',
  'graph':
    '<circle cx="6" cy="7" r="2"/><circle cx="18" cy="7" r="2"/><circle cx="12" cy="18" r="2"/><path d="M8 8l3 8"/><path d="M16 8l-3 8"/><path d="M8 7h8"/>',
  'gateway':
    '<path d="M4 7h16v10H4z"/><path d="M8 11h8"/><path d="M8 14h4"/><path d="M7 17v3"/><path d="M17 17v3"/><path d="M12 4v3"/>',
  'logs': '<path d="M6 4h12v16H6z"/><path d="M9 8h6"/><path d="M9 12h6"/><path d="M9 16h4"/>',
  'list':
    '<path d="M8 6h12"/><path d="M8 12h12"/><path d="M8 18h12"/><path d="M4 6h.01"/><path d="M4 12h.01"/><path d="M4 18h.01"/>',
  'lock': '<rect x="5" y="10" width="14" height="10" rx="2"/><path d="M8 10V7a4 4 0 0 1 8 0v3"/>',
  'moreVertical': '<path d="M12 5h.01"/><path d="M12 12h.01"/><path d="M12 19h.01"/>',
  'pin': '<path d="M12 17v5"/><path d="M8 4h8l-1 6 3 3v2H6v-2l3-3-1-6z"/>',
  'edit': '<path d="M12 20h9"/><path d="M16.5 3.5a2.1 2.1 0 0 1 3 3L8 18l-4 1 1-4 11.5-11.5z"/>',
  'download': '<path d="M12 3v12"/><path d="M7 10l5 5 5-5"/><path d="M5 21h14"/>',
  'help':
    '<circle cx="12" cy="12" r="9"/><path d="M9.6 9a2.6 2.6 0 1 1 4.4 1.9c-.9.7-1.6 1.2-1.6 2.6"/><path d="M12 17h.01"/>',
  'memory':
    '<rect x="5" y="6" width="14" height="12" rx="2"/><path d="M8 3v3"/><path d="M12 3v3"/><path d="M16 3v3"/><path d="M8 18v3"/><path d="M12 18v3"/><path d="M16 18v3"/>',
  'mic':
    '<path d="M12 3a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><path d="M12 19v3"/>',
  'paperclip':
    '<path d="M21.4 11.6l-8.5 8.5a5 5 0 0 1-7.1-7.1l9.2-9.2a3.5 3.5 0 0 1 5 5l-9.2 9.2a2 2 0 0 1-2.8-2.8l8.5-8.5"/>',
  'play': '<path d="M8 5v14l11-7z"/>',
  'pulse': '<path d="M4 13h4l2-6 4 12 2-6h4"/>',
  'search': '<circle cx="11" cy="11" r="7"/><path d="M20 20l-3.5-3.5"/>',
  'settings':
    '<circle cx="12" cy="12" r="3"/><path d="M19 12a7 7 0 0 0-.1-1l2-1.5-2-3.5-2.4 1a7 7 0 0 0-1.7-1L14.5 3h-5l-.3 3a7 7 0 0 0-1.7 1L5.1 6l-2 3.5 2 1.5a7 7 0 0 0 0 2l-2 1.5 2 3.5 2.4-1a7 7 0 0 0 1.7 1l.3 3h5l.3-3a7 7 0 0 0 1.7-1l2.4 1 2-3.5-2-1.5a7 7 0 0 0 .1-1z"/>',
  'shield':
    '<path d="M12 3l7 4v5c0 4.5-3 7.5-7 9-4-1.5-7-4.5-7-9V7l7-4z"/><path d="M9 12l2 2 4-5"/>',
  'spark':
    '<path d="M12 2l1.8 6.2L20 10l-6.2 1.8L12 18l-1.8-6.2L4 10l6.2-1.8L12 2z"/><path d="M5 18l.8 2.2L8 21l-2.2.8L5 24l-.8-2.2L2 21l2.2-.8L5 18z"/>',
  'terminal': '<path d="M4 5h16v14H4z"/><path d="M7 9l3 3-3 3"/><path d="M12 15h5"/>',
  'chevrons-left': '<path d="M17 7l-5 5 5 5"/><path d="M11 7l-5 5 5 5"/>',
  'chevrons-right': '<path d="M7 7l5 5-5 5"/><path d="M13 7l5 5-5 5"/>',
  'panelLeft': '<rect x="4" y="5" width="16" height="14" rx="1"/><path d="M9 5v14"/>',
  'panelRight': '<rect x="4" y="5" width="16" height="14" rx="1"/><path d="M15 5v14"/>',
};

export function icon(name, label = '') {
  const body = paths[name] || paths.spark;
  return `<svg class="svg-icon" aria-hidden="${label ? 'false' : 'true'}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round">${label ? `<title>${label}</title>` : ''}${body}</svg>`;
}

export function hydrateStaticIcons(root = document) {
  root.querySelectorAll('[data-icon]').forEach((element) => {
    element.innerHTML = icon(element.dataset.icon);
  });
}
