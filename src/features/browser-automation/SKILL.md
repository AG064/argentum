# Browser Automation Skill

Automated web browser interactions using Playwright.

## Installation

```bash
cd ~/.openclaw/workspace/skills/browser-automation
npm install
npx playwright install chromium
```

## Commands

| Command | Description |
|---------|-------------|
| `browser install` | Install Playwright and Chromium |
| `browser navigate <url>` | Go to URL |
| `browser click <selector>` | Click element |
| `browser fill <selector> <text>` | Fill input field |
| `browser type <selector> <text>` | Type text |
| `browser wait <selector> [ms]` | Wait for element |
| `browser screenshot [filename]` | Take screenshot |
| `browser title` | Get page title |
| `browser url` | Get current URL |
| `browser text <selector>` | Get element text |
| `browser eval <js>` | Run JavaScript |
| `browser close` | Close browser |

## Examples

### Login to Moodle
```bash
browser navigate "https://moodle.edu.ee"
browser click "text=Logi sisse"
browser sleep 2000
browser click "text=Jätka"
browser fill "input[name=username]" "ag064"
browser fill "input[name=password]" "mypassword"
browser eval "document.getElementById('loginbtn').click()"
browser sleep 5000
browser screenshot "logged-in.png"
```

### Fill Form
```bash
browser navigate "https://example.com/form"
browser fill "#email" "test@example.com"
browser fill "#message" "Hello world"
browser click "button[type=submit]"
```

### Extract Data
```bash
browser navigate "https://news.com"
browser eval "Array.from(document.querySelectorAll('h2')).map(h => h.innerText)"
```

## Troubleshooting

- **Browser won't start**: Run `npx playwright install chromium`
- **Element not found**: Use `browser screenshot` to debug
- **Page loads slowly**: Add `browser sleep 2000` after navigate

## Requirements

- Node.js 18+
- Chromium browser (installed by Playwright)
