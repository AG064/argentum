/**
 * Browser Automation Feature
 * 
 * Provides automated web browser interactions using Playwright.
 * 
 * Usage:
 *   argentum browser navigate <url>
 *   argentum browser click <selector>
 *   argentum browser fill <selector> <text>
 *   argentum browser screenshot [filename]
 *   argentum browser install
 * 
 * Requires: npm install && npx playwright install chromium
 */

import type { Feature } from '../types';

export const browserAutomation: Feature = {
  name: 'browser-automation',
  description: 'Automated web browser interactions using Playwright',
  commands: [
    {
      name: 'browser',
      description: 'Browser automation CLI',
      subcommands: [
        { name: 'install', description: 'Install Playwright Chromium' },
        { name: 'navigate', description: 'Go to URL', args: '<url>' },
        { name: 'click', description: 'Click element', args: '<selector>' },
        { name: 'fill', description: 'Fill input', args: '<selector> <text>' },
        { name: 'type', description: 'Type text', args: '<selector> <text>' },
        { name: 'screenshot', description: 'Take screenshot', args: '[filename]' },
        { name: 'wait', description: 'Wait for element', args: '<selector> [ms]' },
        { name: 'sleep', description: 'Wait', args: '<ms>' },
        { name: 'eval', description: 'Run JavaScript', args: '<js>' },
        { name: 'title', description: 'Get page title' },
        { name: 'url', description: 'Get current URL' },
        { name: 'text', description: 'Get element text', args: '<selector>' },
        { name: 'close', description: 'Close browser' },
      ],
    },
  ],
  scripts: {
    'browser-install': 'npm install && npx playwright install chromium',
  },
};

export default browserAutomation;
