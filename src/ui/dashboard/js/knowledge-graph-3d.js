/**
 * AG-Claw Dashboard - Knowledge Graph 3D Visualization
 * 
 * Interactive 3D force-directed graph visualization using 3d-force-graph.
 * Reads nodes/edges from AG-Claw's existing MemoryGraph API.
 */

// Node type to color mapping
const TYPE_COLORS = {
  person: '#6366f1',
  organization: '#22c55e',
  location: '#f59e0b',
  concept: '#ec4899',
  event: '#8b5cf6',
  document: '#14b8a6',
  project: '#f97316',
  task: '#06b6d4',
  skill: '#84cc16',
  agent: '#a855f7',
  memory: '#64748b',
  default: '#6b7280',
};

// CSS class for entity types (used in legend)
const TYPE_CLASSES = {
  person: 'badge-primary',
  organization: 'badge-success',
  location: 'badge-warning',
  concept: 'badge-accent',
  event: 'badge-purple',
  document: 'badge-teal',
  project: 'badge-orange',
  task: 'badge-info',
  skill: 'badge-lime',
  agent: 'badge-violet',
  memory: 'badge-slate',
  default: 'badge-default',
};

let graphInstance = null;
let highlightNodes = new Set();
let highlightLinks = new Set();
let currentGraphData = { nodes: [], links: [] };

/**
 * Get color for entity type
 */
function getTypeColor(type) {
  return TYPE_COLORS[type?.toLowerCase()] || TYPE_COLORS.default;
}

/**
 * Get CSS class for entity type badge
 */
function getTypeClass(type) {
  return TYPE_CLASSES[type?.toLowerCase()] || TYPE_CLASSES.default;
}

/**
 * Fetch graph data from the API
 */
async function fetchGraphData() {
  try {
    const response = await API.get('/features/knowledge-graph');
    if (response && response.entities && response.relationships) {
      return transformToGraphData(response.entities, response.relationships);
    }
    // Fallback: try direct graph endpoint
    const graphResponse = await API.get('/graph');
    if (graphResponse && graphResponse.entities) {
      return transformToGraphData(graphResponse.entities, graphResponse.relationships || []);
    }
  } catch (error) {
    console.error('[KG-3D] Failed to fetch graph data:', error);
  }
  // Return mock data for demo
  return generateMockGraphData();
}

/**
 * Transform API response to 3D graph format
 */
function transformToGraphData(entities, relationships) {
  const nodes = entities.map((entity) => ({
    id: entity.id,
    name: entity.name,
    type: entity.type || 'default',
    tags: entity.tags || [],
    properties: entity.properties || {},
    val: 1,
    color: getTypeColor(entity.type),
  }));

  const links = relationships.map((rel) => {
    const sourceId = typeof rel.source === 'object' ? rel.source?.id : rel.sourceId;
    const targetId = typeof rel.target === 'object' ? rel.target?.id : rel.targetId;
    return {
      source: sourceId || '',
      target: targetId || '',
      type: rel.type || 'related',
      weight: rel.weight || 1.0,
    };
  }).filter(link => link.source && link.target);

  return { nodes, links };
}

/**
 * Generate mock graph data for demonstration
 */
function generateMockGraphData() {
  const nodes = [
    { id: 'ag-claw', name: 'AG-Claw', type: 'project', tags: ['ai', 'assistant'], properties: { version: '0.2.0' }, val: 10, color: TYPE_COLORS.project },
    { id: 'coder', name: 'Coder Agent', type: 'agent', tags: ['coding', 'development'], properties: { status: 'active' }, val: 5, color: TYPE_COLORS.agent },
    { id: 'researcher', name: 'Researcher Agent', type: 'agent', tags: ['research'], properties: { status: 'active' }, val: 4, color: TYPE_COLORS.agent },
    { id: 'foreman', name: 'Foreman Agent', type: 'agent', tags: ['orchestration'], properties: { status: 'active' }, val: 3, color: TYPE_COLORS.agent },
    { id: 'memory-graph', name: 'Memory Graph', type: 'concept', tags: ['memory', 'graph'], properties: { edges: 156 }, val: 6, color: TYPE_COLORS.concept },
    { id: 'knowledge-graph', name: 'Knowledge Graph', type: 'concept', tags: ['knowledge', 'entities'], properties: { entities: 89 }, val: 5, color: TYPE_COLORS.concept },
    { id: 'skills', name: 'Skills System', type: 'concept', tags: ['extensibility'], properties: { count: 12 }, val: 4, color: TYPE_COLORS.concept },
    { id: 'dashboard', name: 'Web Dashboard', type: 'project', tags: ['ui', 'monitoring'], properties: { pages: 11 }, val: 4, color: TYPE_COLORS.project },
    { id: 'aleks', name: 'Aleksey (AG)', type: 'person', tags: ['creator', 'user'], properties: { timezone: 'Europe/Tallinn' }, val: 8, color: TYPE_COLORS.person },
    { id: 'semantic-mem', name: 'Semantic Memory', type: 'memory', tags: ['semantic', 'embeddings'], properties: {}, val: 3, color: TYPE_COLORS.memory },
    { id: 'episodic-mem', name: 'Episodic Memory', type: 'memory', tags: ['sessions'], properties: {}, val: 3, color: TYPE_COLORS.memory },
    { id: 'openclaw', name: 'OpenClaw', type: 'organization', tags: ['platform', 'framework'], properties: {}, val: 7, color: TYPE_COLORS.organization },
    { id: '3d-viz', name: '3D Visualization', type: 'skill', tags: ['visualization', '3d'], properties: {}, val: 2, color: TYPE_COLORS.skill },
    { id: 'web-dashboard', name: 'Web Dashboard', type: 'skill', tags: ['ui', 'typescript'], properties: {}, val: 3, color: TYPE_COLORS.skill },
  ];

  const links = [
    { source: 'ag-claw', target: 'coder', type: 'uses', weight: 1.0 },
    { source: 'ag-claw', target: 'researcher', type: 'uses', weight: 1.0 },
    { source: 'ag-claw', target: 'foreman', type: 'uses', weight: 1.0 },
    { source: 'ag-claw', target: 'memory-graph', type: 'implements', weight: 0.9 },
    { source: 'ag-claw', target: 'knowledge-graph', type: 'implements', weight: 0.9 },
    { source: 'ag-claw', target: 'skills', type: 'uses', weight: 0.8 },
    { source: 'ag-claw', target: 'dashboard', type: 'includes', weight: 0.7 },
    { source: 'aleks', target: 'ag-claw', type: 'created', weight: 1.0 },
    { source: 'aleks', target: 'openclaw', type: 'uses', weight: 1.0 },
    { source: 'openclaw', target: 'ag-claw', type: 'powers', weight: 1.0 },
    { source: 'memory-graph', target: 'semantic-mem', type: 'stores', weight: 0.8 },
    { source: 'memory-graph', target: 'episodic-mem', type: 'stores', weight: 0.8 },
    { source: 'knowledge-graph', target: 'memory-graph', type: 'extends', weight: 0.7 },
    { source: 'skills', target: '3d-viz', type: 'includes', weight: 0.5 },
    { source: 'skills', target: 'web-dashboard', type: 'includes', weight: 0.6 },
    { source: 'dashboard', target: 'web-dashboard', type: 'uses', weight: 0.8 },
    { source: '3d-viz', target: 'knowledge-graph', type: 'visualizes', weight: 0.7 },
  ];

  return { nodes, links };
}

/**
 * Initialize the 3D Knowledge Graph
 */
async function initKnowledgeGraph3D(containerId) {
  if (!containerId) containerId = 'kg3dContainer';

  const container = document.getElementById(containerId);
  if (!container) {
    console.error('[KG-3D] Container not found:', containerId);
    return;
  }

  // Load 3d-force-graph from CDN if not already loaded
  await ensure3DForceGraphLoaded();

  // Fetch graph data
  const graphData = await fetchGraphData();
  currentGraphData = graphData;

  // Update stats
  updateKGStats(graphData);

  if (graphData.nodes.length === 0) {
    container.innerHTML = `
      <div style="display: flex; flex-direction: column; align-items: center; justify-content: center; height: 100%; color: var(--color-text-muted);">
        <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" style="opacity: 0.5; margin-bottom: 16px;">
          <circle cx="12" cy="12" r="10"/>
          <path d="M12 8v4M12 16h.01"/>
        </svg>
        <p style="margin: 0; font-size: 14px;">No knowledge graph data available.</p>
        <p style="margin: 8px 0 0 0; font-size: 12px; opacity: 0.7;">Add entities and relationships to see the 3D visualization.</p>
      </div>
    `;
    return;
  }

  // Clear container
  container.innerHTML = '';

  // Create 3D graph
  const Graph = window.3DForceGraph;

  graphInstance = Graph({
    containerId: containerId,
    nodeId: 'id',
    nodeLabel: 'name',
    nodeVal: 'val',
    nodeColor: 'color',
    linkSource: 'source',
    linkTarget: 'target',
    linkLabel: 'type',
    linkColor: function(link) {
      if (highlightNodes.size > 0) {
        const sourceId = typeof link.source === 'object' ? link.source.id : link.source;
        const targetId = typeof link.target === 'object' ? link.target.id : link.target;
        return highlightNodes.has(sourceId) || highlightNodes.has(targetId)
          ? 'rgba(99, 102, 241, 0.8)'
          : 'rgba(107, 114, 128, 0.3)';
      }
      return 'rgba(99, 102, 241, 0.4)';
    },
    linkWidth: function(link) {
      return Math.max(0.5, link.weight || 1) * 2;
    },
    linkDirectionalParticles: 2,
    linkDirectionalParticleWidth: 2,
    linkDirectionalParticleColor: function() { return 'rgba(99, 102, 241, 0.6)'; },
    linkDirectionalParticleSpeed: 0.005,
    onNodeClick: handleNodeClick,
    onNodeRightClick: handleNodeRightClick,
    onNodeHover: handleNodeHover,
    onLinkHover: handleLinkHover,
    enableNodeDrag: true,
    enableNavigationControls: true,
    showNavInfo: true,
    nodeThreeObject: function(node) { return createNode3DObject(node); },
    nodeThreeObjectExtend: true,
  })(graphData);

  // Configure camera
  if (graphInstance.camera) {
    graphInstance.camera().position({ x: 0, y: 0, z: 200 });
  }

  console.log('[KG-3D] Initialized with', graphData.nodes.length, 'nodes and', graphData.links.length, 'edges');
}

/**
 * Update KG statistics display
 */
function updateKGStats(graphData) {
  const nodeCountEl = document.getElementById('kgNodeCount');
  const edgeCountEl = document.getElementById('kgEdgeCount');
  if (nodeCountEl) nodeCountEl.textContent = graphData.nodes.length;
  if (edgeCountEl) edgeCountEl.textContent = graphData.links.length;
}

/**
 * Create 3D object for a node
 */
function createNode3DObject(node) {
  const THREE = window.THREE;
  if (!THREE) return null;

  // Create a sphere geometry with the node's color
  const geometry = new THREE.SphereGeometry(node.val * 2 + 4, 32, 32);
  const material = new THREE.MeshPhongMaterial({
    color: new THREE.Color(node.color),
    transparent: true,
    opacity: 0.9,
    shininess: 80,
  });

  const mesh = new THREE.Mesh(geometry, material);

  // Add glow effect
  const glowGeometry = new THREE.SphereGeometry(node.val * 2 + 8, 32, 32);
  const glowMaterial = new THREE.MeshBasicMaterial({
    color: new THREE.Color(node.color),
    transparent: true,
    opacity: 0.2,
  });
  const glowMesh = new THREE.Mesh(glowGeometry, glowMaterial);
  mesh.add(glowMesh);

  return mesh;
}

/**
 * Handle node click - show details panel
 */
function handleNodeClick(node, event) {
  console.log('[KG-3D] Node clicked:', node);
  showNodeDetails(node);
}

/**
 * Handle node right-click - center on node
 */
function handleNodeRightClick(node, event) {
  event.preventDefault();
  if (graphInstance) {
    graphInstance.centerAt(node.id, 500);
  }
}

/**
 * Handle node hover - highlight connected nodes
 */
function handleNodeHover(node, prevNode) {
  // Reset highlights
  if (!node) {
    highlightNodes.clear();
    highlightLinks.clear();
    if (graphInstance) {
      graphInstance.graphData(graphInstance.graphData()); // Refresh
    }
    hideHoverInfo();
    return;
  }

  // Find connected nodes
  highlightNodes.clear();
  highlightNodes.add(node.id);

  const graphData = graphInstance?.graphData();
  if (graphData?.links) {
    for (const link of graphData.links) {
      const sourceId = typeof link.source === 'object' ? link.source.id : link.source;
      const targetId = typeof link.target === 'object' ? link.target.id : link.target;
      if (sourceId === node.id) highlightNodes.add(targetId);
      if (targetId === node.id) highlightNodes.add(sourceId);
    }
  }

  showHoverInfo(node);
  if (graphInstance) {
    graphInstance.graphData(graphInstance.graphData()); // Refresh
  }
}

/**
 * Handle link hover
 */
function handleLinkHover(link, prevLink) {
  if (!link) {
    hideHoverInfo();
    return;
  }

  const sourceId = typeof link.source === 'object' ? link.source.id : link.source;
  const targetId = typeof link.target === 'object' ? link.target.id : link.target;
  const sourceNode = graphInstance?.graphData()?.nodes?.find((n) => n.id === sourceId);
  const targetNode = graphInstance?.graphData()?.nodes?.find((n) => n.id === targetId);

  showLinkInfo(link, sourceNode, targetNode);
}

/**
 * Show hover info tooltip
 */
function showHoverInfo(node) {
  let hoverEl = document.getElementById('kg3dHoverInfo');
  if (!hoverEl) {
    hoverEl = document.createElement('div');
    hoverEl.id = 'kg3dHoverInfo';
    hoverEl.style.cssText = `
      position: fixed;
      top: 20px;
      right: 20px;
      background: var(--color-surface);
      border: 1px solid var(--color-border);
      border-radius: 8px;
      padding: 12px 16px;
      z-index: 1000;
      max-width: 280px;
      font-size: 13px;
      box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
      pointer-events: none;
      opacity: 0;
      transition: opacity 0.2s;
    `;
    document.body.appendChild(hoverEl);
  }

  const tagsHtml = node.tags.length > 0
    ? `<div style="margin-top: 8px; display: flex; flex-wrap: wrap; gap: 4px;">
        ${node.tags.slice(0, 3).map(tag => `<span style="background: rgba(99, 102, 241, 0.2); color: var(--color-accent); padding: 2px 6px; border-radius: 4px; font-size: 10px;">${tag}</span>`).join('')}
      </div>`
    : '';

  hoverEl.innerHTML = `
    <div style="font-weight: 600; margin-bottom: 8px; display: flex; align-items: center; gap: 8px;">
      <span style="width: 10px; height: 10px; border-radius: 50%; background: ${node.color}; display: inline-block;"></span>
      ${node.name}
    </div>
    <div style="color: var(--color-text-muted); font-size: 11px; text-transform: uppercase; letter-spacing: 0.5px;">
      ${node.type}
    </div>
    ${tagsHtml}
  `;
  hoverEl.style.opacity = '1';
}

/**
 * Show link info
 */
function showLinkInfo(link, source, target) {
  let hoverEl = document.getElementById('kg3dHoverInfo');
  if (!hoverEl) {
    hoverEl = document.createElement('div');
    hoverEl.id = 'kg3dHoverInfo';
    hoverEl.style.cssText = `
      position: fixed;
      top: 20px;
      right: 20px;
      background: var(--color-surface);
      border: 1px solid var(--color-border);
      border-radius: 8px;
      padding: 12px 16px;
      z-index: 1000;
      max-width: 280px;
      font-size: 13px;
      box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
      pointer-events: none;
      opacity: 0;
      transition: opacity 0.2s;
    `;
    document.body.appendChild(hoverEl);
  }

  hoverEl.innerHTML = `
    <div style="font-weight: 600; margin-bottom: 4px; color: var(--color-accent);">
      ${link.type}
    </div>
    <div style="display: flex; align-items: center; gap: 8px;">
      <span style="color: var(--color-text-muted);">${source?.name || 'Unknown'}</span>
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M5 12h14M12 5l7 7-7 7"/>
      </svg>
      <span style="color: var(--color-text-muted);">${target?.name || 'Unknown'}</span>
    </div>
    ${link.weight ? `<div style="margin-top: 4px; font-size: 11px; color: var(--color-text-muted);">Weight: ${(link.weight * 100).toFixed(0)}%</div>` : ''}
  `;
  hoverEl.style.opacity = '1';
}

/**
 * Hide hover info
 */
function hideHoverInfo() {
  const hoverEl = document.getElementById('kg3dHoverInfo');
  if (hoverEl) {
    hoverEl.style.opacity = '0';
  }
}

/**
 * Show detailed node information panel
 */
function showNodeDetails(node) {
  // Remove existing panel
  const existingPanel = document.getElementById('kg3dDetailsPanel');
  if (existingPanel) existingPanel.remove();

  const panel = document.createElement('div');
  panel.id = 'kg3dDetailsPanel';
  panel.style.cssText = `
    position: fixed;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    background: var(--color-surface);
    border: 1px solid var(--color-border);
    border-radius: 12px;
    padding: 24px;
    z-index: 1001;
    min-width: 320px;
    max-width: 480px;
    max-height: 80vh;
    overflow-y: auto;
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.3);
  `;

  // Create overlay
  const overlay = document.createElement('div');
  overlay.style.cssText = `
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.5);
    z-index: 1000;
  `;
  overlay.onclick = function() {
    panel.remove();
    overlay.remove();
  };

  const tagsHtml = node.tags.length > 0
    ? `<div style="margin-top: 12px; display: flex; flex-wrap: wrap; gap: 6px;">
        ${node.tags.map(tag => `<span style="background: rgba(99, 102, 241, 0.15); color: var(--color-accent); padding: 4px 10px; border-radius: 6px; font-size: 12px;">${tag}</span>`).join('')}
       </div>`
    : '';

  const propsHtml = Object.keys(node.properties).length > 0
    ? `<div style="margin-top: 16px;">
        <div style="font-size: 11px; text-transform: uppercase; letter-spacing: 0.5px; color: var(--color-text-muted); margin-bottom: 8px;">Properties</div>
        <div style="background: var(--color-bg); border-radius: 6px; padding: 12px; font-family: monospace; font-size: 12px;">
          ${Object.entries(node.properties).map(([k, v]) => `<div><span style="color: var(--color-accent);">${k}:</span> ${JSON.stringify(v)}</div>`).join('')}
        </div>
       </div>`
    : '';

  panel.innerHTML = `
    <div style="display: flex; justify-content: space-between; align-items: flex-start; margin-bottom: 16px;">
      <div style="display: flex; align-items: center; gap: 12px;">
        <div style="width: 48px; height: 48px; border-radius: 50%; background: ${node.color}22; display: flex; align-items: center; justify-content: center;">
          <span style="width: 20px; height: 20px; border-radius: 50%; background: ${node.color};"></span>
        </div>
        <div>
          <div style="font-size: 18px; font-weight: 600;">${node.name}</div>
          <div style="font-size: 12px; color: var(--color-text-muted); text-transform: uppercase; letter-spacing: 0.5px;">
            <span class="badge ${getTypeClass(node.type)}">${node.type}</span>
          </div>
        </div>
      </div>
      <button id="kg3dDetailsClose" style="background: none; border: none; cursor: pointer; color: var(--color-text-muted); padding: 4px;">
        <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M18 6L6 18M6 6l12 12"/>
        </svg>
      </button>
    </div>
    ${tagsHtml}
    ${propsHtml}
    <div style="margin-top: 16px; display: flex; gap: 8px;">
      <button id="kg3dCenterNode" style="flex: 1; background: var(--color-accent); color: white; border: none; padding: 8px 12px; border-radius: 6px; cursor: pointer; font-size: 13px;">
        Center View
      </button>
      <button id="kg3dFocusNeighbors" style="flex: 1; background: var(--color-surface-elevated); color: var(--color-text); border: 1px solid var(--color-border); padding: 8px 12px; border-radius: 6px; cursor: pointer; font-size: 13px;">
        Focus Neighbors
      </button>
    </div>
  `;

  document.body.appendChild(overlay);
  document.body.appendChild(panel);

  // Event listeners
  document.getElementById('kg3dDetailsClose').onclick = function() {
    panel.remove();
    overlay.remove();
  };

  document.getElementById('kg3dCenterNode').onclick = function() {
    if (graphInstance) {
      graphInstance.centerAt(node.id, 500);
      graphInstance.zoomToFit(500);
    }
    panel.remove();
    overlay.remove();
  };

  document.getElementById('kg3dFocusNeighbors').onclick = function() {
    focusOnNeighbors(node);
    panel.remove();
    overlay.remove();
  };
}

/**
 * Focus camera on node's neighbors
 */
function focusOnNeighbors(node) {
  if (!graphInstance) return;

  highlightNodes.clear();
  highlightNodes.add(node.id);

  const graphData = graphInstance.graphData();
  if (graphData?.links) {
    for (const link of graphData.links) {
      const sourceId = typeof link.source === 'object' ? link.source.id : link.source;
      const targetId = typeof link.target === 'object' ? link.target.id : link.target;
      if (sourceId === node.id) highlightNodes.add(targetId);
      if (targetId === node.id) highlightNodes.add(sourceId);
    }
  }

  graphInstance.graphData(graphData);
  graphInstance.centerAt(node.id, 500);
  graphInstance.zoomToFit(500);
}

/**
 * Load 3D Force Graph library from CDN
 */
async function ensure3DForceGraphLoaded() {
  // Check if already loaded
  if (window.3DForceGraph) return Promise.resolve();

  return new Promise(function(resolve, reject) {
    // Load Three.js first
    if (!window.THREE) {
      const threeScript = document.createElement('script');
      threeScript.src = 'https://unpkg.com/three@0.160.0/build/three.min.js';
      threeScript.onload = function() { load3DForceGraph(resolve, reject); };
      threeScript.onerror = reject;
      document.head.appendChild(threeScript);
    } else {
      load3DForceGraph(resolve, reject);
    }
  });
}

/**
 * Load 3D Force Graph
 */
function load3DForceGraph(resolve, reject) {
  const graphScript = document.createElement('script');
  graphScript.src = 'https://unpkg.com/3d-force-graph@1.2.3/dist/3d-force-graph.min.js';
  graphScript.onload = resolve;
  graphScript.onerror = reject;
  document.head.appendChild(graphScript);
}

/**
 * Update graph with new data
 */
function updateGraphData(data) {
  if (graphInstance) {
    graphInstance.graphData(data);
    currentGraphData = data;
    updateKGStats(data);
  }
}

/**
 * Refresh the graph (re-render)
 */
function refreshGraph() {
  if (graphInstance) {
    graphInstance.graphData(graphInstance.graphData());
  }
}

/**
 * Zoom to fit all nodes
 */
function zoomToFit() {
  if (graphInstance) {
    graphInstance.zoomToFit(500);
  }
}

/**
 * Center on a specific node
 */
function centerOnNode(nodeId) {
  if (graphInstance) {
    graphInstance.centerAt(nodeId, 500);
  }
}

/**
 * Dispose of the 3D graph
 */
function disposeGraph() {
  if (graphInstance) {
    if (graphInstance._destructor) graphInstance._destructor();
    graphInstance = null;
  }
  highlightNodes.clear();
  highlightLinks.clear();
  currentGraphData = { nodes: [], links: [] };
}

/**
 * Handle search input
 */
function handleKGSearch(query) {
  if (!query || query.length < 2) {
    // Show all nodes
    if (graphInstance) {
      graphInstance.graphData(currentGraphData);
    }
    return;
  }

  const queryLower = query.toLowerCase();
  const filteredNodes = currentGraphData.nodes.filter(node =>
    node.name.toLowerCase().includes(queryLower) ||
    node.type.toLowerCase().includes(queryLower) ||
    node.tags.some(tag => tag.toLowerCase().includes(queryLower))
  );

  const filteredNodeIds = new Set(filteredNodes.map(n => n.id));
  const filteredLinks = currentGraphData.links.filter(link => {
    const sourceId = typeof link.source === 'object' ? link.source.id : link.source;
    const targetId = typeof link.target === 'object' ? link.target.id : link.target;
    return filteredNodeIds.has(sourceId) && filteredNodeIds.has(targetId);
  });

  if (graphInstance) {
    graphInstance.graphData({ nodes: filteredNodes, links: filteredLinks });
  }
}

/**
 * Handle type filter
 */
function handleKGTypeFilter(type) {
  if (!type) {
    // Show all nodes
    if (graphInstance) {
      graphInstance.graphData(currentGraphData);
    }
    return;
  }

  const filteredNodes = currentGraphData.nodes.filter(node => node.type === type);
  const filteredNodeIds = new Set(filteredNodes.map(n => n.id));
  const filteredLinks = currentGraphData.links.filter(link => {
    const sourceId = typeof link.source === 'object' ? link.source.id : link.source;
    const targetId = typeof link.target === 'object' ? link.target.id : link.target;
    return filteredNodeIds.has(sourceId) && filteredNodeIds.has(targetId);
  });

  if (graphInstance) {
    graphInstance.graphData({ nodes: filteredNodes, links: filteredLinks });
  }
}

/**
 * Show add entity modal
 */
function showAddEntityModal() {
  if (typeof Components !== 'undefined' && Components.prompt) {
    Components.prompt(
      'Add Entity',
      'Enter entity name:',
      '',
      function(name) {
        if (name && name.trim()) {
          Components.toast('Entity creation would connect to API here', 'info');
        }
      }
    );
  } else {
    alert('Entity creation: Connect to KnowledgeGraphFeature API to add entities');
  }
}

// Export for use in HTML
window.KnowledgeGraph3D = {
  init: initKnowledgeGraph3D,
  update: updateGraphData,
  refresh: refreshGraph,
  zoomToFit: zoomToFit,
  centerOnNode: centerOnNode,
  dispose: disposeGraph,
  fetchData: fetchGraphData,
  handleSearch: handleKGSearch,
  handleTypeFilter: handleKGTypeFilter,
};
