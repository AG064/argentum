// SPDX-License-Identifier: MIT
// Copyright (c) 2024-2026 AG064
/**
 * Argentum Skills Catalog
 *
 * Browsable catalog of official skills from Anthropic and Codex.
 * All skills use the agentskills.io SKILL.md format and are
 * compatible with Argentum's skills-loader.
 *
 * Sources:
 *   - Anthropic: github.com/anthropics/skills (16 official skills)
 *   - Codex:     github.com/openai/skills (40+ curated + system skills)
 */

export type SkillSource = 'anthropic' | 'codex' | 'installed' | 'argentum';
export type SkillCategory =
  | 'document'
  | 'design'
  | 'code'
  | 'devops'
  | 'security'
  | 'ai'
  | 'collaboration'
  | 'testing'
  | 'deployment'
  | 'general';

export interface CatalogSkill {
/** Unique slug that must be a valid folder name */
  name: string;
  /** Short description from the skill's SKILL.md frontmatter */
  description: string;
  /** Longer description or use-case guidance */
  longDescription?: string;
  source: SkillSource;
  /** e.g. "anthropic/skills" or "openai/skills" */
  repo?: string;
  /** Path inside the repo e.g. "skills/docx" or "skills/.curated/figma-use" */
  repoPath?: string;
  category: SkillCategory;
  /** GitHub URL to the skill's SKILL.md */
  url?: string;
  /** Tags for filtering */
  tags: string[];
  /** Whether Argentum already has this capability built-in */
  builtinNote?: string;
}

// ─── Anthropic Official Skills ────────────────────────────────────────────────

// ─── Argentum Bundled Skills ─────────────────────────────────────────────────────
// These SKILL.md guidance files are present in Argentum's feature source tree.
// Runtime availability still depends on feature enablement and external tools.
// The catalog also includes upstream Anthropic/Codex entries for optional install.

export const ARGENTUM_BUNDLED_SKILLS: CatalogSkill[] = [
  {
    name: 'browser-automation',
    description:
      'Automate browser navigation, form entry, screenshots, and page inspection through the bundled Playwright feature guidance.',
    source: 'argentum',
    category: 'testing',
    tags: ['browser', 'playwright', 'automation', 'testing'],
    builtinNote:
      'Feature guidance is bundled with Argentum. Playwright and Chromium must be installed before browser commands are available.',
  },
  {
    name: 'computer-control',
    description:
      'Control supported desktop environments through screenshots and permission-gated mouse and keyboard actions.',
    source: 'argentum',
    category: 'general',
    tags: ['desktop', 'computer-use', 'automation', 'vision'],
    builtinNote:
      'Feature guidance is bundled with Argentum. Computer control is disabled by default and may require platform tools.',
  },
  {
    name: 'skill-loader',
    description:
      'Load bundled feature SKILL.md files and inject their guidance into enabled agent contexts.',
    source: 'argentum',
    category: 'ai',
    tags: ['skills', 'context', 'agent', 'loader'],
    builtinNote:
      'Bundled with Argentum and used by the TypeScript agent runtime for enabled feature directories.',
  },
  {
    name: 'youtube-shorts',
    description:
      'Generate vertical video clips from YouTube sources and publish to supported destinations when dependencies and credentials are configured.',
    source: 'argentum',
    category: 'deployment',
    tags: ['youtube', 'video', 'shorts', 'media'],
    builtinNote:
      'Feature guidance is bundled with Argentum. yt-dlp, ffmpeg, and provider credentials are required; the feature is disabled by default.',
  },
];

export const ANTHROPIC_SKILLS: CatalogSkill[] = [
  {
    name: 'theme-factory',
    description:
    'Apply consistent, professional styling to any artifact, including slides, docs, reports, and HTML pages. Choose from 10 pre-set themes with curated color palettes and font pairings, or generate a custom theme on the fly.',
    source: 'anthropic',
    repo: 'anthropics/skills',
    repoPath: 'skills/theme-factory',
    url: 'https://github.com/anthropics/skills/tree/main/skills/theme-factory',
    category: 'design',
    tags: ['theme', 'design', 'styling', 'color', 'fonts'],
  },
  {
    name: 'frontend-design',
    description:
      'Design and implement frontend UIs following modern best practices. Covers component architecture, accessibility, responsive layouts, CSS methodologies, and design system patterns.',
    source: 'anthropic',
    repo: 'anthropics/skills',
    repoPath: 'skills/frontend-design',
    url: 'https://github.com/anthropics/skills/tree/main/skills/frontend-design',
    category: 'design',
    tags: ['frontend', 'ui', 'css', 'html', 'accessibility'],
  },
  {
    name: 'canvas-design',
    description:
      'Generate interactive art and visualizations using the HTML Canvas API. Produce algorithmic patterns, generative graphics, data visualizations, and custom canvas-based applications.',
    source: 'anthropic',
    repo: 'anthropics/skills',
    repoPath: 'skills/canvas-design',
    url: 'https://github.com/anthropics/skills/tree/main/skills/canvas-design',
    category: 'design',
    tags: ['canvas', 'graphics', 'art', 'visualization', 'generative'],
  },
  {
    name: 'algorithmic-art',
    description:
      'Create algorithmic and generative art using JavaScript, Canvas API, and SVG. Generate procedural patterns, fractal visualizations, and creative coding projects with real-time rendering.',
    source: 'anthropic',
    repo: 'anthropics/skills',
    repoPath: 'skills/algorithmic-art',
    url: 'https://github.com/anthropics/skills/tree/main/skills/algorithmic-art',
    category: 'design',
    tags: ['art', 'generative', 'creative', 'canvas', 'svg'],
  },
  {
    name: 'webapp-testing',
    description:
      'Test web applications end-to-end using Playwright. Write automated browser tests, capture screenshots, inspect console logs, and debug UI failures in realistic browser environments.',
    source: 'anthropic',
    repo: 'anthropics/skills',
    repoPath: 'skills/webapp-testing',
    url: 'https://github.com/anthropics/skills/tree/main/skills/webapp-testing',
    category: 'testing',
    tags: ['testing', 'playwright', 'e2e', 'browser', 'automation'],
  },
  {
    name: 'mcp-builder',
    description:
      'Build high-quality MCP (Model Context Protocol) servers in Python (FastMCP) or TypeScript (MCP SDK). Covers tool design, authentication, error handling, and publishing MCP packages.',
    source: 'anthropic',
    repo: 'anthropics/skills',
    repoPath: 'skills/mcp-builder',
    url: 'https://github.com/anthropics/skills/tree/main/skills/mcp-builder',
    category: 'ai',
    tags: ['mcp', 'api', 'tool', 'server', 'integration'],
  },
  {
    name: 'skill-creator',
    description:
      'Create new skills and iteratively improve existing ones. Write SKILL.md files, run evals to test skill quality, benchmark triggering accuracy, and optimize descriptions for better activation.',
    source: 'anthropic',
    repo: 'anthropics/skills',
    repoPath: 'skills/skill-creator',
    url: 'https://github.com/anthropics/skills/tree/main/skills/skill-creator',
    category: 'ai',
    tags: ['skill', 'creation', 'development', 'evals'],
  },
  {
    name: 'brand-guidelines',
    description:
      "Apply brand guidelines consistently in creative work. Ensures colors, fonts, logos, voice, and visual style match your organization's brand standards across all generated artifacts.",
    source: 'anthropic',
    repo: 'anthropics/skills',
    repoPath: 'skills/brand-guidelines',
    url: 'https://github.com/anthropics/skills/tree/main/skills/brand-guidelines',
    category: 'design',
    tags: ['brand', 'design', 'guidelines', 'consistency'],
  },
  {
    name: 'internal-comms',
    description:
    'Draft professional internal communications such as team announcements, project updates, and org-wide memos. Uses company-specific voice and format guidelines for clear, consistent internal messaging.',
    source: 'anthropic',
    repo: 'anthropics/skills',
    repoPath: 'skills/internal-comms',
    url: 'https://github.com/anthropics/skills/tree/main/skills/internal-comms',
    category: 'collaboration',
    tags: ['comms', 'internal', 'announcement', 'memos'],
  },
  {
    name: 'doc-coauthoring',
    description:
      'Collaborate on documents with AI co-authorship. Provides structured workflows for writing, editing, and iterating on documents with consistent voice, style, and structure.',
    source: 'anthropic',
    repo: 'anthropics/skills',
    repoPath: 'skills/doc-coauthoring',
    url: 'https://github.com/anthropics/skills/tree/main/skills/doc-coauthoring',
    category: 'document',
    tags: ['writing', 'collaboration', 'document', 'coauthor'],
  },
  {
    name: 'slack-gif-creator',
    description:
      'Create animated GIFs optimized for Slack. Build branded reaction GIFs, team announcements, and Slack-specific visual content using Canvas API and optimized export pipelines.',
    source: 'anthropic',
    repo: 'anthropics/skills',
    repoPath: 'skills/slack-gif-creator',
    url: 'https://github.com/anthropics/skills/tree/main/skills/slack-gif-creator',
    category: 'design',
    tags: ['slack', 'gif', 'animation', 'social'],
  },
  {
    name: 'web-artifacts-builder',
    description:
    'Build rich web artifacts such as interactive visualizations, data dashboards, and polished HTML presentations. Creates self-contained HTML pages with embedded CSS and JavaScript.',
    source: 'anthropic',
    repo: 'anthropics/skills',
    repoPath: 'skills/web-artifacts-builder',
    url: 'https://github.com/anthropics/skills/tree/main/skills/web-artifacts-builder',
    category: 'design',
    tags: ['html', 'visualization', 'dashboard', 'web'],
  },
  {
    name: 'claude-api',
    description:
      'Guide for integrating with the Anthropic Claude API. Covers API authentication, rate limits, model selection, prompt engineering, streaming responses, and error handling.',
    source: 'anthropic',
    repo: 'anthropics/skills',
    repoPath: 'skills/claude-api',
    url: 'https://github.com/anthropics/skills/tree/main/skills/claude-api',
    category: 'ai',
    tags: ['api', 'claude', 'anthropic', 'integration'],
  },
];

// ─── Codex Curated Skills ────────────────────────────────────────────────────

export const CODEX_CURATED_SKILLS: CatalogSkill[] = [
  {
    name: 'figma-use',
    description:
    'MANDATORY prerequisite for every Figma plugin API call. Covers JavaScript execution in Figma files via the Plugin API: create, edit, or delete nodes, set variables, build components, modify auto-layout, and inspect file structure programmatically.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/figma-use',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/figma-use',
    category: 'design',
    tags: ['figma', 'design', 'ui', 'plugin'],
  },
  {
    name: 'figma-generate-design',
    description:
    'Build full pages and screens in Figma from code. Use with figma-use to discover design system components, import them, and assemble screens incrementally using the Figma Plugin API.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/figma-generate-design',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/figma-generate-design',
    category: 'design',
    tags: ['figma', 'design', 'generation', 'ui'],
  },
  {
    name: 'figma-create-design-system-rules',
    description:
      'Create and document design system rules and component specifications in Figma. Define design tokens, component variants, interaction states, and documentation standards.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/figma-create-design-system-rules',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/figma-create-design-system-rules',
    category: 'design',
    tags: ['figma', 'design-system', 'tokens', 'components'],
  },
  {
    name: 'figma-create-new-file',
    description:
    'Initialize new Figma files with proper structure, including pages, frames, grids, and design system foundations. Sets up a clean starting point for new design projects.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/figma-create-new-file',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/figma-create-new-file',
    category: 'design',
    tags: ['figma', 'design', 'setup'],
  },
  {
    name: 'figma-implement-design',
    description:
      'Implement designs from Figma in code. Converts Figma frames and components to production-ready HTML, CSS, React, or other frontend code with pixel-perfect accuracy.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/figma-implement-design',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/figma-implement-design',
    category: 'design',
    tags: ['figma', 'implementation', 'html', 'css', 'react'],
  },
  {
    name: 'figma',
    description:
      'General-purpose Figma integration. Covers file browsing, component inspection, design handoff workflows, and Figma API interactions for reading design assets and specifications.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/figma',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/figma',
    category: 'design',
    tags: ['figma', 'design', 'handoff'],
  },
  {
    name: 'gh-address-comments',
    description:
      'Address GitHub PR review comments systematically. Fetches open comments, drafts responses, and proposes code fixes for each unresolved review item.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/gh-address-comments',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/gh-address-comments',
    category: 'collaboration',
    tags: ['github', 'pr', 'review', 'comments'],
  },
  {
    name: 'gh-fix-ci',
    description:
      'Debug and fix failing GitHub Actions PR checks. Uses gh CLI to inspect checks and logs, summarize failure context, draft a fix plan, and implement fixes after explicit approval.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/gh-fix-ci',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/gh-fix-ci',
    category: 'devops',
    tags: ['github', 'ci', 'actions', 'debugging'],
  },
  {
    name: 'vercel-deploy',
    description:
      'Deploy applications and websites to Vercel. Handles preview and production deployments, domain configuration, and environment variables via the Vercel CLI.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/vercel-deploy',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/vercel-deploy',
    category: 'deployment',
    tags: ['vercel', 'deployment', 'hosting', 'frontend'],
  },
  {
    name: 'cloudflare-deploy',
    description:
      'Deploy to Cloudflare using Workers, Pages, and related services. Covers Workers AI, Durable Objects, R2 storage, D1 database, and Cloudflare configuration.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/cloudflare-deploy',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/cloudflare-deploy',
    category: 'deployment',
    tags: ['cloudflare', 'workers', 'deployment', 'serverless'],
  },
  {
    name: 'netlify-deploy',
    description:
      'Deploy to Netlify with drag-and-drop, CLI, or Git integration. Configures build settings, redirects, headers, and environment variables for static and Jamstack sites.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/netlify-deploy',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/netlify-deploy',
    category: 'deployment',
    tags: ['netlify', 'deployment', 'hosting', 'jamstack'],
  },
  {
    name: 'render-deploy',
    description:
      'Deploy to Render with auto-scaling web services, background workers, cron jobs, and managed databases. Covers PostgreSQL, Redis, and environment configuration.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/render-deploy',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/render-deploy',
    category: 'deployment',
    tags: ['render', 'deployment', 'hosting', 'backend'],
  },
  {
    name: 'playwright',
    description:
    'Automate real browsers from the terminal for navigation, form filling, snapshots, screenshots, and data extraction via playwright-cli or bundled wrapper scripts.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/playwright',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/playwright',
    category: 'testing',
    tags: ['playwright', 'browser', 'automation', 'testing', 'screenshots'],
  },
  {
    name: 'playwright-interactive',
    description:
      'Interactive Playwright debugging and exploration. Step through browser automation scripts, inspect DOM state, and debug flaky tests interactively.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/playwright-interactive',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/playwright-interactive',
    category: 'testing',
    tags: ['playwright', 'debugging', 'interactive', 'testing'],
  },
  {
    name: 'security-best-practices',
    description:
      'Language and framework-specific security best-practice reviews. Supports Python, JavaScript, and TypeScript. Identifies OWASP Top 10 issues, injection vulnerabilities, and insecure defaults.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/security-best-practices',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/security-best-practices',
    category: 'security',
    tags: ['security', 'owasp', 'best-practices', 'review'],
  },
  {
    name: 'security-threat-model',
    description:
      'Create threat models and security architecture reviews. Identifies attack surfaces, models threat actors, and recommends mitigations for software systems.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/security-threat-model',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/security-threat-model',
    category: 'security',
    tags: ['security', 'threat-model', 'architecture'],
  },
  {
    name: 'security-ownership-map',
    description:
      'Map security ownership and responsibility across codebases. Identifies who owns which components, their security responsibilities, and generates accountability matrices.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/security-ownership-map',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/security-ownership-map',
    category: 'security',
    tags: ['security', 'ownership', 'accountability'],
  },
  {
    name: 'sentry',
    description:
      'Inspect Sentry issues and events, summarize recent production errors, and pull health metrics via the Sentry CLI. Read-only queries for observability and debugging.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/sentry',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/sentry',
    category: 'devops',
    tags: ['sentry', 'observability', 'errors', 'monitoring'],
  },
  {
    name: 'linear',
    description:
      'Manage issues, projects, and team workflows in Linear. Create, update, and track tickets; manage sprints; and sync project status using Linear MCP.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/linear',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/linear',
    category: 'collaboration',
    tags: ['linear', 'project-management', 'issues', 'tickets'],
  },
  {
    name: 'notion-knowledge-capture',
    description:
      'Capture conversations and decisions into structured Notion pages. Convert chats, notes, and decisions into wiki entries, how-tos, FAQs, and team documentation with proper linking.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/notion-knowledge-capture',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/notion-knowledge-capture',
    category: 'collaboration',
    tags: ['notion', 'knowledge', 'documentation', 'wiki'],
  },
  {
    name: 'notion-meeting-intelligence',
    description:
      'Process meeting notes and transcripts in Notion. Extract action items, decisions, and key takeaways, then organize them into structured meeting records.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/notion-meeting-intelligence',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/notion-meeting-intelligence',
    category: 'collaboration',
    tags: ['notion', 'meetings', 'notes', 'transcripts'],
  },
  {
    name: 'notion-research-documentation',
    description:
      'Organize research findings and technical documentation in Notion. Structure research projects, citation databases, and technical specs with proper taxonomy.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/notion-research-documentation',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/notion-research-documentation',
    category: 'collaboration',
    tags: ['notion', 'research', 'documentation'],
  },
  {
    name: 'notion-spec-to-implementation',
    description:
      'Convert Notion specs and design documents into implementation-ready code. Extract requirements from Notion pages and translate them into code tasks.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/notion-spec-to-implementation',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/notion-spec-to-implementation',
    category: 'collaboration',
    tags: ['notion', 'spec', 'implementation', 'workflow'],
  },
  {
    name: 'openai-docs',
    description:
      'Get authoritative, up-to-date guidance on OpenAI products and APIs. Use OpenAI docs MCP tools for API references, model selection, and migration guidance with official citations.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/openai-docs',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/openai-docs',
    category: 'ai',
    tags: ['openai', 'api', 'docs', 'documentation'],
  },
  {
    name: 'pdf',
    description:
    'Process PDF files for extraction, merging, splitting, watermarking, form filling, OCR, and text analysis. Uses Python libraries and command-line tools.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/pdf',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/pdf',
    category: 'document',
    tags: ['pdf', 'document', 'ocr'],
    builtinNote: 'Argentum already bundles argentum-pdf.',
  },
  {
    name: 'cli-creator',
    description:
      'Build production-ready command-line tools in Python, Node, or Go. Covers argument parsing, help text, shell completions, config files, and distribution packaging.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/cli-creator',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/cli-creator',
    category: 'code',
    tags: ['cli', 'tool', 'command-line', 'python', 'go'],
  },
  {
    name: 'aspnet-core',
    description:
      'Build, review, and architect ASP.NET Core web applications. Covers Blazor, Razor Pages, MVC, Minimal APIs, SignalR, gRPC, middleware, DI, authentication, testing, and deployment.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/aspnet-core',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/aspnet-core',
    category: 'code',
    tags: ['dotnet', 'aspnet', 'csharp', 'web', 'api'],
  },
  {
    name: 'winui-app',
    description:
      'Build Windows native applications with WinUI 3 and the Windows App SDK. Create Fluent Design UIs, integrate with Windows Runtime APIs, and package MSIX applications.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/winui-app',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/winui-app',
    category: 'code',
    tags: ['windows', 'winui', 'csharp', 'desktop', 'fluent'],
  },
  {
    name: 'chatgpt-apps',
    description:
      'Build ChatGPT plugin and GPT apps with OpenAI API integration. Covers manifest files, OpenAPI specs, auth flows, streaming, and the Assistants API.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/chatgpt-apps',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/chatgpt-apps',
    category: 'ai',
    tags: ['openai', 'chatgpt', 'plugins', 'gpt', 'assistants'],
  },
  {
    name: 'jupyter-notebook',
    description:
      'Work with Jupyter notebooks programmatically. Create, edit, and execute notebooks; extract code cells; and build reproducible data analysis workflows.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/jupyter-notebook',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/jupyter-notebook',
    category: 'code',
    tags: ['jupyter', 'notebook', 'data', 'python', 'analysis'],
  },
  {
    name: 'define-goal',
    description:
      'Break down ambiguous requests into clear, actionable goals before implementation. Creates structured task lists with acceptance criteria and estimated complexity.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/define-goal',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/define-goal',
    category: 'general',
    tags: ['planning', 'goals', 'task-definition'],
  },
  {
    name: 'screenshot',
    description:
      'Capture screenshots of web pages and applications for documentation and testing. Uses headless browser rendering with configurable viewport sizes and wait conditions.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/screenshot',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/screenshot',
    category: 'testing',
    tags: ['screenshot', 'browser', 'testing', 'documentation'],
  },
  {
    name: 'speech',
    description:
      'Convert text to speech and speech to text using cloud APIs. Supports OpenAI Whisper, Google Cloud Speech, Azure Speech, and browser Web Speech API.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/speech',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/speech',
    category: 'ai',
    tags: ['speech', 'tts', 'stt', 'audio', 'transcription'],
  },
  {
    name: 'transcribe',
    description:
      'Transcribe audio and video files to text using Whisper and other ASR engines. Handles various formats, speaker diarization, and timestamped output.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/transcribe',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/transcribe',
    category: 'ai',
    tags: ['transcription', 'audio', 'whisper', 'video'],
  },
  {
    name: 'hatch-pet',
    description:
      'Personal project assistant for software development. Helps plan, track, and execute personal coding projects with structured workflows and progress tracking.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/hatch-pet',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/hatch-pet',
    category: 'general',
    tags: ['personal', 'project', 'workflow', 'productivity'],
  },
  {
    name: 'migrate-to-codex',
    description:
      'Migrate existing projects and workflows from other AI coding tools (Cursor, Copilot, etc.) to Codex. Covers environment setup, key bindings, and workflow translation.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/migrate-to-codex',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/migrate-to-codex',
    category: 'general',
    tags: ['migration', 'setup', 'onboarding'],
  },
  {
    name: 'yeet',
    description:
    'Quick-launch skill for common dev tasks such as scaffolding projects, spinning up dev servers, running tests, and deploying from a single command.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/yeet',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/yeet',
    category: 'general',
    tags: ['scaffolding', 'launch', 'workflow', 'productivity'],
  },
  {
    name: 'figma-code-connect-components',
    description:
      'Connect Figma components to code implementations. Generates TypeScript types and component wrappers that stay in sync with Figma design tokens.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/figma-code-connect-components',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/figma-code-connect-components',
    category: 'design',
    tags: ['figma', 'components', 'typescript', 'codegen'],
  },
  {
    name: 'figma-create-library',
    description:
      'Build and publish Figma component libraries. Sets up design token exports, documentation, and versioning for team-shared design systems.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.curated/figma-create-library',
    url: 'https://github.com/openai/skills/tree/main/skills/.curated/figma-create-library',
    category: 'design',
    tags: ['figma', 'design-system', 'library', 'components'],
  },
];

// ─── Codex System Skills ──────────────────────────────────────────────────────

export const CODEX_SYSTEM_SKILLS: CatalogSkill[] = [
  {
    name: 'imagegen',
    description:
      'Generate images using OpenAI image generation models (DALL-E). Create AI artwork, illustrations, UI mockups, and visual assets from text prompts.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.system/imagegen',
    url: 'https://github.com/openai/skills/tree/main/skills/.system/imagegen',
    category: 'design',
    tags: ['image', 'dall-e', 'ai', 'art', 'generation'],
  },
  {
    name: 'plugin-creator',
    description:
      'Create Codex plugins and extensions. Scaffold plugin projects, define manifest files, implement tool endpoints, and publish to the plugin marketplace.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.system/plugin-creator',
    url: 'https://github.com/openai/skills/tree/main/skills/.system/plugin-creator',
    category: 'ai',
    tags: ['plugin', 'codex', 'extension', 'marketplace'],
  },
  {
    name: 'skill-installer',
    description:
      'Install skills from GitHub repositories and the skills marketplace. Resolves skill dependencies, validates SKILL.md format, and manages skill versions.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.system/skill-installer',
    url: 'https://github.com/openai/skills/tree/main/skills/.system/skill-installer',
    category: 'general',
    tags: ['skill', 'installer', 'management'],
  },
  {
    name: 'openai-docs-system',
    description:
      'Internal skill for accessing OpenAI documentation and API references. Provides authoritative guidance on OpenAI models, APIs, and best practices.',
    source: 'codex',
    repo: 'openai/skills',
    repoPath: 'skills/.system/openai-docs',
    url: 'https://github.com/openai/skills/tree/main/skills/.system/openai-docs',
    category: 'ai',
    tags: ['openai', 'docs', 'api', 'reference'],
  },
];

// ─── All Skills ───────────────────────────────────────────────────────────────

export const ALL_CATALOG_SKILLS: CatalogSkill[] = [
  ...ARGENTUM_BUNDLED_SKILLS,
  ...ANTHROPIC_SKILLS,
  ...CODEX_CURATED_SKILLS,
  ...CODEX_SYSTEM_SKILLS,
];

export const SKILLS_BY_SOURCE: Record<SkillSource, CatalogSkill[]> = {
  argentum: ARGENTUM_BUNDLED_SKILLS,
  anthropic: ANTHROPIC_SKILLS,
  codex: [...CODEX_CURATED_SKILLS, ...CODEX_SYSTEM_SKILLS],
  installed: [], // Populated at runtime from list_installed_skills
};

export const CATEGORIES: { id: SkillCategory; label: string }[] = [
  { id: 'document', label: 'Documents' },
  { id: 'design', label: 'Design' },
  { id: 'code', label: 'Code' },
  { id: 'devops', label: 'DevOps' },
  { id: 'security', label: 'Security' },
  { id: 'ai', label: 'AI & APIs' },
  { id: 'collaboration', label: 'Collaboration' },
  { id: 'testing', label: 'Testing' },
  { id: 'deployment', label: 'Deployment' },
  { id: 'general', label: 'General' },
];

/** Search across all catalog skills by name, description, or tags */
export function searchCatalogSkills(query: string, skills: CatalogSkill[]): CatalogSkill[] {
  if (!query.trim()) return skills;
  const q = query.toLowerCase();
  return skills.filter(
    (s) =>
      s.name.toLowerCase().includes(q) ||
      s.description.toLowerCase().includes(q) ||
      s.tags.some((t) => t.toLowerCase().includes(q)),
  );
}
