#!/usr/bin/env node
/**
 * Browser Automation Script using Playwright
 * State is kept between commands until 'close'
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

async function closeBrowser() {
  if (browser) {
    await browser.close();
    browser = null;
    page = null;
  }
}

async function runCommand(args) {
  const [command, ...rest] = args;
  const { page } = await ensureBrowser();

  switch (command) {
    case 'navigate': {
      const url = rest[0];
      if (!url) throw new Error('Usage: browser navigate <url>');
      await page.goto(url, { waitUntil: 'domcontentloaded', timeout: 30000 });
      await page.waitForTimeout(2000); // Wait for full render
      console.log('OK:', url);
      break;
    }

    case 'click': {
      const selector = rest[0];
      if (!selector) throw new Error('Usage: browser click <selector>');
      await page.waitForSelector(selector, { timeout: 10000 });
      await page.click(selector);
      await page.waitForTimeout(1000);
      console.log('OK: clicked', selector);
      break;
    }

    case 'fill': {
      const selector = rest[0];
      const text = rest.slice(1).join(' ');
      if (!selector || text === undefined) throw new Error('Usage: browser fill <selector> <text>');
      await page.waitForSelector(selector, { timeout: 10000 });
      await page.fill(selector, text);
      console.log('OK: filled', selector);
      break;
    }

    case 'type': {
      const selector = rest[0];
      const text = rest.slice(1).join(' ');
      if (!selector || text === undefined) throw new Error('Usage: browser type <selector> <text>');
      await page.waitForSelector(selector, { timeout: 10000 });
      await page.type(selector, text);
      console.log('OK: typed into', selector);
      break;
    }

    case 'text': {
      const selector = rest[0];
      if (!selector) throw new Error('Usage: browser text <selector>');
      await page.waitForSelector(selector, { timeout: 10000 });
      const text = await page.textContent(selector);
      console.log(text);
      return text;
    }

    case 'screenshot': {
      const filename = rest[0] || `screenshot-${Date.now()}.png`;
      await page.screenshot({ path: filename, fullPage: true });
      console.log('OK:', filename);
      break;
    }

    case 'eval': {
      const js = rest.join(' ');
      if (!js) throw new Error('Usage: browser eval <javascript>');
      const result = await page.evaluate(js);
      console.log(JSON.stringify(result));
      return result;
    }

    case 'wait': {
      const selector = rest[0];
      const timeout = parseInt(rest[1]) || 10000;
      await page.waitForSelector(selector, { timeout });
      console.log('OK: found', selector);
      break;
    }

    case 'url': {
      console.log(page.url());
      return page.url();
    }

    case 'title': {
      const title = await page.title();
      console.log(title);
      return title;
    }

    case 'close': {
      await closeBrowser();
      console.log('OK: closed');
      break;
    }

    case 'sleep': {
      const ms = parseInt(rest[0]) || 1000;
      await page.waitForTimeout(ms);
      console.log('OK: slept', ms, 'ms');
      break;
    }

    default:
      throw new Error(`Unknown: ${command}`);
  }
}

if (require.main === module) {
  const args = process.argv.slice(2);
  if (args.length === 0) {
    console.log('Browser CLI: navigate, click, fill, type, text, screenshot, eval, wait, url, title, close, sleep');
    process.exit(1);
  }

  runCommand(args)
    .then(() => process.exit(0))
    .catch(err => {
      console.error('ERROR:', err.message);
      process.exit(1);
    });
}

module.exports = { runCommand, ensureBrowser, closeBrowser };
