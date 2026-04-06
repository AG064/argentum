#!/usr/bin/env node
/**
 * Browser Automation CLI
 * Usage: browser <command> [args...]
 * 
 * Commands:
 *   install          - Install Playwright and Chromium
 *   navigate <url>  - Go to URL
 *   click <sel>     - Click element
 *   fill <sel> <text> - Fill input
 *   type <sel> <text> - Type text
 *   wait <sel> [ms]   - Wait for element
 *   screenshot [file] - Take screenshot
 *   title            - Get page title
 *   url              - Get current URL
 *   text <sel>       - Get element text
 *   eval <js>        - Run JavaScript
 *   sleep <ms>        - Wait
 *   close            - Close browser
 */

const { chromium } = require('playwright');

let browser = null;
let page = null;

async function ensureBrowser() {
  if (!browser) {
    browser = await chromium.launch({ headless: true });
    page = await browser.newPage();
  }
  return { browser, page };
}

async function runCommand(args) {
  const [command, ...rest] = args;
  const { page } = await ensureBrowser();

  switch (command) {
    case 'install':
      console.log('Installing Playwright Chromium...');
      const { execSync } = require('child_process');
      execSync('npx playwright install chromium', { stdio: 'inherit' });
      console.log('Done!');
      break;

    case 'navigate': {
      const url = rest[0];
      if (!url) throw new Error('Usage: browser navigate <url>');
      await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30000 });
      await page.waitForTimeout(2000);
      console.log('OK:', url);
      break;
    }

    case 'click': {
      const selector = rest[0];
      if (!selector) throw new Error('Usage: browser click <selector>');
      await page.waitForSelector(selector, { timeout: 10000 }).catch(() => {});
      await page.click(selector).catch(e => console.log('Click warning:', e.message.split('\n')[0]));
      await page.waitForTimeout(1000);
      console.log('OK: clicked', selector);
      break;
    }

    case 'fill': {
      const selector = rest[0];
      const text = rest.slice(1).join(' ');
      if (!selector || text === undefined) throw new Error('Usage: browser fill <selector> <text>');
      await page.waitForSelector(selector, { timeout: 10000 }).catch(() => {});
      await page.fill(selector, text).catch(e => console.log('Fill warning:', e.message.split('\n')[0]));
      console.log('OK: filled', selector);
      break;
    }

    case 'type': {
      const selector = rest[0];
      const text = rest.slice(1).join(' ');
      if (!selector || text === undefined) throw new Error('Usage: browser type <selector> <text>');
      await page.waitForSelector(selector, { timeout: 10000 }).catch(() => {});
      await page.type(selector, text).catch(e => console.log('Type warning:', e.message.split('\n')[0]));
      console.log('OK: typed into', selector);
      break;
    }

    case 'text': {
      const selector = rest[0];
      if (!selector) throw new Error('Usage: browser text <selector>');
      await page.waitForSelector(selector, { timeout: 10000 }).catch(() => {});
      const text = await page.textContent(selector).catch(() => 'N/A');
      console.log(text);
      return text;
    }

    case 'screenshot': {
      const filename = rest[0] || `/tmp/browser-${Date.now()}.png`;
      await page.screenshot({ path: filename, fullPage: true });
      console.log('OK:', filename);
      break;
    }

    case 'eval': {
      const js = rest.join(' ');
      if (!js) throw new Error('Usage: browser eval <javascript>');
      const result = await page.evaluate(js).catch(e => ({ error: e.message }));
      console.log(JSON.stringify(result, null, 2));
      return result;
    }

    case 'wait': {
      const selector = rest[0];
      const timeout = parseInt(rest[1]) || 10000;
      if (!selector) throw new Error('Usage: browser wait <selector> [timeout_ms]');
      await page.waitForSelector(selector, { timeout }).catch(e => console.log('Wait timeout:', e.message.split('\n')[0]));
      console.log('OK: found', selector);
      break;
    }

    case 'sleep': {
      const ms = parseInt(rest[0]) || 1000;
      await page.waitForTimeout(ms);
      console.log('OK: slept', ms, 'ms');
      break;
    }

    case 'url':
      console.log(page.url());
      return page.url();

    case 'title':
      const title = await page.title();
      console.log(title);
      return title;

    case 'close':
      if (browser) {
        await browser.close();
        browser = null;
        page = null;
      }
      console.log('OK: closed');
      break;

    default:
      throw new Error(`Unknown command: ${command}`);
  }
}

// CLI
if (require.main === module) {
  const args = process.argv.slice(2);
  
  if (args.length === 0 || args[0] === '--help' || args[0] === '-h') {
    console.log('Browser Automation CLI');
    console.log('');
    console.log('Usage: browser <command> [args...]');
    console.log('');
    console.log('Commands:');
    console.log('  install              Install Playwright Chromium');
    console.log('  navigate <url>      Go to URL');
    console.log('  click <selector>     Click element');
    console.log('  fill <sel> <text>   Fill input');
    console.log('  type <sel> <text>   Type text');
    console.log('  wait <sel> [ms]      Wait for element');
    console.log('  screenshot [file]    Take screenshot');
    console.log('  title               Get page title');
    console.log('  url                 Get current URL');
    console.log('  text <selector>      Get element text');
    console.log('  eval <js>           Run JavaScript');
    console.log('  sleep <ms>           Wait');
    console.log('  close               Close browser');
    console.log('');
    console.log('Example:');
    console.log('  browser install');
    console.log('  browser navigate "https://example.com"');
    console.log('  browser screenshot');
    console.log('  browser close');
    process.exit(args[0] === '--help' || args[0] === '-h' ? 0 : 1);
  }

  runCommand(args)
    .then(() => process.exit(0))
    .catch(err => {
      console.error('ERROR:', err.message);
      process.exit(1);
    });
}

module.exports = { runCommand, ensureBrowser };
