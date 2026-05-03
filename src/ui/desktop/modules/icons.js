const paths = {
  agent: '<path d="M12 3l7 4v6c0 4-3 7-7 8-4-1-7-4-7-8V7l7-4z"/><path d="M9 12h6"/><path d="M10 16h4"/>',
  bell: '<path d="M18 8a6 6 0 1 0-12 0c0 7-3 7-3 9h18c0-2-3-2-3-9"/><path d="M10 21h4"/>',
  blocks: '<rect x="4" y="4" width="7" height="7" rx="1"/><rect x="13" y="4" width="7" height="7" rx="1"/><rect x="4" y="13" width="7" height="7" rx="1"/><rect x="13" y="13" width="7" height="7" rx="1"/>',
  channel: '<path d="M5 12h4"/><path d="M15 12h4"/><circle cx="12" cy="12" r="3"/><circle cx="4" cy="12" r="2"/><circle cx="20" cy="12" r="2"/>',
  chat: '<path d="M5 5h14v10H8l-3 3V5z"/><path d="M8 9h8"/><path d="M8 12h5"/>',
  externalLink: '<path d="M14 4h6v6"/><path d="M20 4l-9 9"/><path d="M20 14v4a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h4"/>',
  graph: '<circle cx="6" cy="7" r="2"/><circle cx="18" cy="7" r="2"/><circle cx="12" cy="18" r="2"/><path d="M8 8l3 8"/><path d="M16 8l-3 8"/><path d="M8 7h8"/>',
  gateway: '<path d="M4 7h16v10H4z"/><path d="M8 11h8"/><path d="M8 14h4"/><path d="M7 17v3"/><path d="M17 17v3"/><path d="M12 4v3"/>',
  logs: '<path d="M6 4h12v16H6z"/><path d="M9 8h6"/><path d="M9 12h6"/><path d="M9 16h4"/>',
  memory: '<rect x="5" y="6" width="14" height="12" rx="2"/><path d="M8 3v3"/><path d="M12 3v3"/><path d="M16 3v3"/><path d="M8 18v3"/><path d="M12 18v3"/><path d="M16 18v3"/>',
  mic: '<path d="M12 3a3 3 0 0 0-3 3v6a3 3 0 0 0 6 0V6a3 3 0 0 0-3-3z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><path d="M12 19v3"/>',
  paperclip: '<path d="M21.4 11.6l-8.5 8.5a5 5 0 0 1-7.1-7.1l9.2-9.2a3.5 3.5 0 0 1 5 5l-9.2 9.2a2 2 0 0 1-2.8-2.8l8.5-8.5"/>',
  play: '<path d="M8 5v14l11-7z"/>',
  pulse: '<path d="M4 13h4l2-6 4 12 2-6h4"/>',
  settings: '<circle cx="12" cy="12" r="3"/><path d="M19 12a7 7 0 0 0-.1-1l2-1.5-2-3.5-2.4 1a7 7 0 0 0-1.7-1L14.5 3h-5l-.3 3a7 7 0 0 0-1.7 1L5.1 6l-2 3.5 2 1.5a7 7 0 0 0 0 2l-2 1.5 2 3.5 2.4-1a7 7 0 0 0 1.7 1l.3 3h5l.3-3a7 7 0 0 0 1.7-1l2.4 1 2-3.5-2-1.5a7 7 0 0 0 .1-1z"/>',
  shield: '<path d="M12 3l7 4v5c0 4.5-3 7.5-7 9-4-1.5-7-4.5-7-9V7l7-4z"/><path d="M9 12l2 2 4-5"/>',
  spark: '<path d="M12 2l1.8 6.2L20 10l-6.2 1.8L12 18l-1.8-6.2L4 10l6.2-1.8L12 2z"/><path d="M5 18l.8 2.2L8 21l-2.2.8L5 24l-.8-2.2L2 21l2.2-.8L5 18z"/>',
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
