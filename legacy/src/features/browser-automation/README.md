# Browser Automation Skill for OpenClaw

Automated web browser interactions using Playwright.

## Installation

```bash
cd ~/.openclaw/workspace/skills/browser-automation
npm install
npx playwright install chromium
```

## Usage

```bash
browser navigate "https://example.com"
browser click "#button"
browser fill "input[name=email]" "test@example.com"
browser screenshot "page.png"
browser close
```

## Commands

| Command | Description |
|---------|-------------|
| `install` | Install Playwright Chromium |
| `navigate <url>` | Go to URL |
| `click <selector>` | Click element |
| `fill <selector> <text>` | Fill input field |
| `type <selector> <text>` | Type text |
| `wait <selector> [ms]` | Wait for element |
| `screenshot [filename]` | Take screenshot |
| `title` | Get page title |
| `url` | Get current URL |
| `text <selector>` | Get element text |
| `eval <javascript>` | Run JavaScript |
| `sleep <ms>` | Wait |
| `close` | Close browser |

## Examples

### Login to Moodle

```bash
browser navigate "https://moodle.edu.ee"
browser click "text=Logi sisse"
browser sleep 2000
browser click "text=Jätka"
browser fill "input[name=username]" "user"
browser fill "input[name=password]" "pass"
browser eval "document.getElementById('loginbtn').click()"
browser sleep 5000
browser screenshot "logged-in.png"
```

### Fill Form and Submit

```bash
browser navigate "https://form.example.com"
browser fill "#email" "test@example.com"
browser fill "#message" "Hello"
browser click "button[type=submit]"
browser sleep 3000
browser screenshot "result.png"
```

## Requirements

- Node.js 18+
- Chromium browser (installed via Playwright)

## License

MIT
