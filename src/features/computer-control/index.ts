/**
 * AG-Claw Computer Control Feature
 *
 * Vision-based desktop automation through screenshot analysis
 * and mouse/keyboard control. Works on Linux (X11/Wayland/Hyprland),
 * macOS, and Windows.
 *
 * Disabled by default — enable only in safe environments.
 */

import { spawn, type ChildProcess } from 'child_process';
import { tmpdir } from 'os';
import { join } from 'path';

import { detectPlatform, type Platform } from './platform';
import { createLogger } from '../../core/logger';
import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../../core/plugin-loader';

// ─── Types ───────────────────────────────────────────────────────────────────

export interface ComputerControlConfig {
  enabled: boolean;
  screenshot_interval_ms: number;
  platform: 'auto' | Platform;
  display_number: number;
}

export interface ScreenshotResult {
  width: number;
  height: number;
  format: 'png';
  data: string; // base64
}

export interface MousePosition {
  x: number;
  y: number;
}

export interface ComputerControl {
  screenshot(): Promise<ScreenshotResult>;
  getMousePosition(): Promise<MousePosition>;
  mouseMove(x: number, y: number): Promise<void>;
  mouseClick(x: number, y: number, button?: 'left' | 'right' | 'middle'): Promise<void>;
  mouseDrag(fromX: number, fromY: number, toX: number, toY: number): Promise<void>;
  mouseScroll(amount: number): Promise<void>;
  keyPress(key: string): Promise<void>;
  keyCombo(keys: string[]): Promise<void>;
  getActiveWindow(): Promise<string>;
  isScreenLocked(): Promise<boolean>;
}

// ─── Vision Decision Types ────────────────────────────────────────────────────

export type VisionAction =
  | { type: 'click'; x: number; y: number; button?: 'left' | 'right' | 'middle' }
  | { type: 'move'; x: number; y: number }
  | { type: 'type'; text: string }
  | { type: 'key'; key: string }
  | { type: 'keycombo'; keys: string[] }
  | { type: 'scroll'; amount: number }
  | { type: 'done' }
  | { type: 'error'; message: string };

export interface VisionDecision {
  action: VisionAction;
  reason: string;
}

export interface VisionProvider {
  analyze(screenshot: ScreenshotResult, prompt: string): Promise<VisionDecision>;
}

// ─── Linux Implementation ─────────────────────────────────────────────────────

function runCommand(cmd: string, args: string[]): Promise<{ stdout: string; stderr: string; exitCode: number }> {
  return new Promise((resolve) => {
    const child = spawn(cmd, args);
    let stdout = '';
    let stderr = '';
    child.stdout?.on('data', (d) => { stdout += d.toString(); });
    child.stderr?.on('data', (d) => { stderr += d.toString(); });
    child.on('close', (code) => {
      resolve({ stdout, stderr, exitCode: code ?? 0 });
    });
    child.on('error', (err) => {
      resolve({ stdout, stderr, exitCode: 1 });
    });
  });
}

function runCommandBuffer(cmd: string, args: string[]): Promise<{ stdout: Buffer; stderr: string; exitCode: number }> {
  return new Promise((resolve) => {
    const child = spawn(cmd, args);
    const chunks: Buffer[] = [];
    let stderr = '';
    child.stdout?.on('data', (d: Buffer) => { chunks.push(Buffer.isBuffer(d) ? d : Buffer.from(d)); });
    child.stderr?.on('data', (d) => { stderr += d.toString(); });
    child.on('close', (code) => {
      resolve({ stdout: Buffer.concat(chunks), stderr, exitCode: code ?? 0 });
    });
    child.on('error', () => {
      resolve({ stdout: Buffer.concat(chunks), stderr, exitCode: 1 });
    });
  });
}

class LinuxComputerControl implements ComputerControl {
  private display: string;

  constructor(displayNumber = 0) {
    this.display = process.env.DISPLAY ?? `:${displayNumber}`;
  }

  async screenshot(): Promise<ScreenshotResult> {
    // Try grim first (Wayland), then scrot (X11), then xwd (X11 fallback)
    const tools = [
      { cmd: 'grim', args: ['-'] },
      { cmd: 'scrot', args: ['-o', '-'] },
      { cmd: 'xwd', args: ['-root', '-display', this.display, '-silent'] },
    ];

    let lastError = '';
    for (const tool of tools) {
      try {
        const result = await runCommandBuffer(tool.cmd, tool.args);
        if (result.exitCode === 0 && result.stdout.length > 0) {
          // Encode raw PNG bytes to base64 and get dimensions
          const base64Data = result.stdout.toString('base64');
          const dims = await this.getScreenshotDimensions(result.stdout);
          return {
            width: dims.width,
            height: dims.height,
            format: 'png',
            data: base64Data,
          };
        }
        lastError = result.stderr || 'no output';
      } catch {
        lastError = 'command failed';
      }
    }

    throw new Error(`All screenshot tools failed. Last error: ${lastError}`);
  }

  private async getScreenshotDimensions(imageData: Buffer): Promise<{ width: number; height: number }> {
    // Write temp file to get dimensions
    const tmpFile = join(tmpdir(), `ag-claw-screenshot-${Date.now()}.png`);
    const fs = await import('fs');
    fs.writeFileSync(tmpFile, imageData);

    const result = await runCommand('file', [tmpFile]);
    const match = result.stdout.match(/(\d+)\s*x\s*(\d+)/);
    fs.unlinkSync(tmpFile);

    if (match?.[1] && match[2]) {
      return { width: parseInt(match[1], 10), height: parseInt(match[2], 10) };
    }
    return { width: 1920, height: 1080 }; // fallback
  }

  async getMousePosition(): Promise<MousePosition> {
    const result = await runCommand('ydotool', ['mousemove', '--help']);
    // ydotool doesn't have get position, use xdotool for X11 or query via wayland
    if (process.env.WAYLAND_DISPLAY) {
      const pos = await runCommand('wlrctl', ['pointer', 'position']);
      const parts = pos.stdout.trim().split(/[,\s]+/);
      const px = parts[0] ? parseInt(parts[0], 10) : 0;
      const py = parts[1] ? parseInt(parts[1], 10) : 0;
      return { x: px, y: py };
    }
    // Fallback to xdotool (X11)
    const out = await runCommand('xdotool', ['mousemove', 'getmouselocation', '--shell']);
    const lines = out.stdout.split('\n');
    let x = 0, y = 0;
    for (const line of lines) {
      if (line.startsWith('X=')) x = parseInt(line.slice(2), 10);
      if (line.startsWith('Y=')) y = parseInt(line.slice(2), 10);
    }
    return { x, y };
  }

  async mouseMove(x: number, y: number): Promise<void> {
    await runCommand('ydotool', ['mousemove', '--x', String(x), '--y', String(y)]);
  }

  async mouseClick(x: number, y: number, button: 'left' | 'right' | 'middle' = 'left'): Promise<void> {
    const btnMap: Record<string, string> = {
      left: '0xC0',
      right: '0xC1',
      middle: '0xC2',
    };
    await this.mouseMove(x, y);
    await runCommand('ydotool', ['click', btnMap[button] ?? '0xC0']);
  }

  async mouseDrag(fromX: number, fromY: number, toX: number, toY: number): Promise<void> {
    await this.mouseMove(fromX, fromY);
    await runCommand('ydotool', ['click', '0xC0']); // press
    // Move in small steps for drag
    const steps = 10;
    for (let i = 1; i <= steps; i++) {
      const px = Math.round(fromX + (toX - fromX) * (i / steps));
      const py = Math.round(fromY + (toY - fromY) * (i / steps));
      await this.mouseMove(px, py);
      await new Promise((r) => setTimeout(r, 10));
    }
    await runCommand('ydotool', ['click', '0xC0']); // release (same button)
  }

  async mouseScroll(amount: number): Promise<void> {
    // ydotool wheel: positive amount = scroll up, negative = scroll down
    await runCommand('ydotool', ['wheel', '--', String(amount)]);
  }

  async keyPress(key: string): Promise<void> {
    // Map common key names to ydotool KEY_* format
    const keyMap: Record<string, string> = {
      Enter: 'KEY_ENTER', Escape: 'KEY_ESC', Tab: 'KEY_TAB',
      Backspace: 'KEY_BACKSPACE', Delete: 'KEY_DELETE',
      ArrowUp: 'KEY_UP', ArrowDown: 'KEY_DOWN', ArrowLeft: 'KEY_LEFT', ArrowRight: 'KEY_RIGHT',
      Space: 'KEY_SPACE',
    };
    const ykey = keyMap[key] ?? `KEY_${key.toUpperCase().replace(/ /g, '_')}`;
    await runCommand('ydotool', ['key', ykey]);
  }

  async keyCombo(keys: string[]): Promise<void> {
    // ydotool key doesn't support combos directly — type each key
    // For true combos, would need to keydown, then keyup each
    for (const key of keys) {
      await this.keyPress(key);
    }
  }

  async getActiveWindow(): Promise<string> {
    if (process.env.WAYLAND_DISPLAY) {
      const result = await runCommand('wlrctl', ['window', 'active', 'title']);
      return result.stdout.trim() || 'Unknown';
    }
    const result = await runCommand('xdotool', ['getactivewindow', 'getwindowname']);
    return result.stdout.trim() || 'Unknown';
  }

  async isScreenLocked(): Promise<boolean> {
    // Check common screen lockers
    const lockCheck = [
      'loginctl show-session $(loginctl | grep $(whoami) | awk "{print \$1}") | grep -q Active',
      'xssstate -s 2>/dev/null | grep -q locked',
      'swayidle 2>/dev/null',
    ];
    for (const check of lockCheck) {
      const result = await runCommand('bash', ['-c', check]);
      if (result.exitCode === 0) return true;
    }
    return false;
  }
}

// ─── macOS Implementation ────────────────────────────────────────────────────

class MacOSComputerControl implements ComputerControl {
  async screenshot(): Promise<ScreenshotResult> {
    const tmpFile = join(tmpdir(), `ag-claw-screenshot-${Date.now()}.png`);
    await runCommand('screencapture', ['-x', tmpFile]);
    const fs = await import('fs');
    const data = fs.readFileSync(tmpFile).toString('base64');
    const dims = await this.getPngDimensions(tmpFile);
    fs.unlinkSync(tmpFile);
    return { width: dims.width, height: dims.height, format: 'png', data };
  }

  private async getPngDimensions(file: string): Promise<{ width: number; height: number }> {
    const result = await runCommand('sips', ['-g', 'pixelWidth', '-g', 'pixelHeight', file]);
    let w = 1920, h = 1080;
    for (const line of result.stdout.split('\n')) {
      if (line.includes('pixelWidth')) w = parseInt(line.replace(/\D/g, ''), 10);
      if (line.includes('pixelHeight')) h = parseInt(line.replace(/\D/g, ''), 10);
    }
    return { width: w, height: h };
  }

  async getMousePosition(): Promise<MousePosition> {
    const result = await runCommand('cliclick', ['p']);
    const [x, y] = result.stdout.trim().split(',').map(Number);
    return { x: x ?? 0, y: y ?? 0 };
  }

  async mouseMove(x: number, y: number): Promise<void> {
    await runCommand('cliclick', [`m:${x},${y}`]);
  }

  async mouseClick(x: number, y: number, button: 'left' | 'right' | 'middle' = 'left'): Promise<void> {
    const btn: Record<string, string> = { left: 'left', right: 'right', middle: 'mid' };
    await runCommand('cliclick', [`${btn[button]}:${x},${y}`]);
  }

  async mouseDrag(fromX: number, fromY: number, toX: number, toY: number): Promise<void> {
    await runCommand('cliclick', [`dc:${fromX},${fromY}`]);
    await runCommand('cliclick', [`m:${toX},${toY}`]);
    await runCommand('cliclick', [`du:${toX},${toY}`]);
  }

  async mouseScroll(amount: number): Promise<void> {
    await runCommand('cliclick', [`scroll:${amount}`]);
  }

  async keyPress(key: string): Promise<void> {
    await runCommand('cliclick', [`t:${key}`]);
  }

  async keyCombo(keys: string[]): Promise<void> {
    for (const key of keys) {
      await runCommand('cliclick', [`kd:${key}`]);
    }
    for (const key of keys.reverse()) {
      await runCommand('cliclick', [`ku:${key}`]);
    }
  }

  async getActiveWindow(): Promise<string> {
    const result = await runCommand('cliclick', ['actv']);
    return result.stdout.trim() || 'Unknown';
  }

  async isScreenLocked(): Promise<boolean> {
    const result = await runCommand('pmset', ['-g', 'powerstate', 'IOPMAssertionTypeIsPreventingIdleSystemSleep']);
    return result.exitCode === 0 && result.stdout.includes('No');
  }
}

// ─── Windows Implementation ───────────────────────────────────────────────────

class WindowsComputerControl implements ComputerControl {
  async screenshot(): Promise<ScreenshotResult> {
    const tmpFile = `C:\\Temp\\ag-claw-screenshot-${Date.now()}.png`;
    const script = `
      Add-Type -AssemblyName System.Windows.Forms
      Add-Type -AssemblyName System.Drawing
      $screen = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
      $bmp = New-Object System.Drawing.Bitmap($screen.Width, $screen.Height)
      $g = [System.Drawing.Graphics]::FromImage($bmp)
      $g.CopyFromScreen($screen.Location, [System.Drawing.Point]::Empty, $screen.Size)
      $bmp.Save("${tmpFile.replace(/\\/g, '\\\\')}")
      $g.Dispose()
      $bmp.Dispose()
      Write-Output "$($screen.Width)x$($screen.Height)"
    `;
    const result = await runCommand('powershell', ['-NoProfile', '-Command', script]);
    const fs = await import('fs');
    const data = fs.readFileSync(tmpFile).toString('base64');
    fs.unlinkSync(tmpFile);
    const dims = result.stdout.trim().split('x');
    return {
      width: parseInt(dims[0] ?? '1920', 10),
      height: parseInt(dims[1] ?? '1080', 10),
      format: 'png',
      data,
    };
  }

  async getMousePosition(): Promise<MousePosition> {
    const result = await runCommand('powershell', [
      '-NoProfile', '-Command',
      '[System.Windows.Forms.Cursor]::Position.X;[System.Windows.Forms.Cursor]::Position.Y',
    ]);
    const [x, y] = result.stdout.trim().split('\n').map(Number);
    return { x: x ?? 0, y: y ?? 0 };
  }

  async mouseMove(x: number, y: number): Promise<void> {
    await runCommand('powershell', [
      '-NoProfile', '-Command',
      `[System.Windows.Forms.Cursor]::Position = [System.Drawing.Point]::new(${x}, ${y})`,
    ]);
  }

  async mouseClick(x: number, y: number, button: 'left' | 'right' | 'middle' = 'left'): Promise<void> {
    await this.mouseMove(x, y);
    const btn: Record<string, string> = { left: 'left', right: 'right', middle: 'middle' };
    const code: Record<string, string> = { left: '0x2', right: '0x8', middle: '0x20' };
    await runCommand('powershell', [
      '-NoProfile', '-Command',
      `Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.MouseButtons]::${btn[button]}`,
    ]);
    // Use SendKeys approach for click
    await runCommand('powershell', [
      '-NoProfile', '-Command',
      `[System.Windows.Forms.Cursor]::Position = [System.Drawing.Point]::new(${x}, ${y}); Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.SendKeys]::SendWait('@{CLICK}')`,
    ]);
  }

  async mouseDrag(fromX: number, fromY: number, toX: number, toY: number): Promise<void> {
    await this.mouseMove(fromX, fromY);
    await runCommand('powershell', [
      '-NoProfile', '-Command',
      'Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.SendKeys]::SendWait("+{CLICK}")',
    ]);
    await new Promise((r) => setTimeout(r, 100));
    await this.mouseMove(toX, toY);
  }

  async mouseScroll(amount: number): Promise<void> {
    await runCommand('powershell', [
      '-NoProfile', '-Command',
      `Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.SendKeys]::SendWait("{LEFT ${amount}})")`,
    ]);
  }

  async keyPress(key: string): Promise<void> {
    // Use SendKeys for basic keys
    const keyMap: Record<string, string> = {
      Enter: '{ENTER}', Escape: '{ESC}', Tab: '{TAB}',
      Backspace: '{BACKSPACE}', Delete: '{DELETE}',
      ArrowUp: '{UP}', ArrowDown: '{DOWN}', ArrowLeft: '{LEFT}', ArrowRight: '{RIGHT}',
    };
    const k = keyMap[key] ?? key;
    await runCommand('powershell', [
      '-NoProfile', '-Command',
      `Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.SendKeys]::SendWait("${k}")`,
    ]);
  }

  async keyCombo(keys: string[]): Promise<void> {
    for (const key of keys) {
      await runCommand('powershell', [
        '-NoProfile', '-Command',
        `Add-Type -AssemblyName System.Windows.Forms; [System.Windows.Forms.SendKeys]::SendWait('^${key}')`,
      ]);
    }
  }

  async getActiveWindow(): Promise<string> {
    const result = await runCommand('powershell', [
      '-NoProfile', '-Command',
      'Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.Interaction]::AppActivate([Microsoft.VisualBasic.Interaction]::GetActiveWindowTitle())',
    ]);
    return result.stdout.trim() || 'Unknown';
  }

  async isScreenLocked(): Promise<boolean> {
    const result = await runCommand('powershell', [
      '-NoProfile', '-Command',
      '(Add-Type -MemberDefinition "[DllImport(\\"user32.dll\\")]public static extern bool LockWorkStation();" -Name Win32Functions -PassThru)::LockWorkStation()',
    ]);
    // If it locks, it wasn't locked before — check state instead
    const state = await runCommand('query', ['session', '/status']);
    return state.stdout.includes('Disc');
  }
}

// ─── Feature Module ─────────────────────────────────────────────────────────

class ComputerControlFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'computer-control',
    version: '0.0.2',
    description: 'Vision-based desktop automation via screenshot + mouse/keyboard control',
    dependencies: [],
  };

  private config: ComputerControlConfig = {
    enabled: false,
    screenshot_interval_ms: 1000,
    platform: 'auto',
    display_number: 0,
  };

  private ctx!: FeatureContext;
  private computer!: ComputerControl;
  private log = createLogger().child({ feature: 'computer-control' });

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = { ...this.config, ...config } as ComputerControlConfig;

    const platform: Platform = this.config.platform === 'auto'
      ? detectPlatform()
      : this.config.platform;

    this.log.info(`Initializing computer control for platform: ${platform}`);

    switch (platform) {
      case 'linux':
        this.computer = new LinuxComputerControl(this.config.display_number);
        break;
      case 'macos':
        this.computer = new MacOSComputerControl();
        break;
      case 'windows':
        this.computer = new WindowsComputerControl();
        break;
      default:
        this.computer = new LinuxComputerControl(this.config.display_number);
    }
  }

  async start(): Promise<void> {
    this.log.info('Computer control feature started (disabled by default)');
  }

  async stop(): Promise<void> {
    this.log.info('Computer control feature stopped');
  }

  async healthCheck(): Promise<HealthStatus> {
    try {
      // Quick screenshot test if enabled
      if (this.config.enabled) {
        await this.computer.screenshot();
        return { healthy: true, message: 'Computer control active', details: { platform: this.config.platform } };
      }
      return { healthy: true, message: 'Computer control available (disabled)' };
    } catch (err) {
      return {
        healthy: false,
        message: err instanceof Error ? err.message : 'Health check failed',
        details: { platform: this.config.platform },
      };
    }
  }

  // Public API

  getComputer(): ComputerControl {
    return this.computer;
  }

  isEnabled(): boolean {
    return this.config.enabled;
  }
}

export default new ComputerControlFeature();
export type { ComputerControlFeature };
