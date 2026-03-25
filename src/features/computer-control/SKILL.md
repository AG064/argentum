# Computer Control Feature

Enables AG-Claw to control desktop environments through vision-based agency.

## Overview

The computer control feature allows AG-Claw to interact with a desktop environment by:
1. Taking screenshots of the current screen
2. Analyzing screenshots with an LLM to understand what's on screen
3. Executing mouse movements, clicks, typing, and keyboard shortcuts
4. Repeating until the task is complete

## Setup

### Linux (Hyprland/Wayland)

```bash
# Install ydotool (mouse/keyboard control — works on Wayland AND X11)
sudo pacman -S ydotool

# Install grim (Wayland screenshot)
sudo pacman -S grim

# For X11: scrot instead
sudo pacman -S scrot

# Enable ydotool daemon (some distros require this)
sudo systemctl enable --now ydotool
```

### Linux (X11 only)

```bash
# Install screenshot tool
sudo pacman -S scrot

# ydotool works on X11 too
sudo pacman -S ydotool

# xdotool (optional, for active window detection)
sudo pacman -S xdotool
```

### macOS

```bash
# Install cliclick (mouse/keyboard control)
brew install cliclick

# screencapture is built-in
```

### Windows

No additional tools required — uses PowerShell and .NET APIs.

## Configuration

In `config/default.yaml`:

```yaml
features:
  computer-control:
    enabled: false          # Disabled by default for safety
    screenshot_interval_ms: 1000
    platform: auto          # auto, linux, macos, windows
    display_number: 0
```

## Usage

AG-Claw can now:
- Play games (Minecraft, etc.)
- Control browsers
- Automate desktop tasks
- Perform system maintenance
- Fill out forms
- Navigate GUI applications

## Safety

**This feature is disabled by default.** Enable only in safe environments:

```yaml
features:
  computer-control:
    enabled: true  # Only enable when you want to use it
```

The feature is designed to be explicitly enabled when needed rather than running continuously.

## Architecture

- `index.ts` — Main feature module with platform-specific implementations
- `platform.ts` — Platform detection utilities
- `agent.ts` — Vision-act loop for autonomous operation
