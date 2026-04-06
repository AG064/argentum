/**
 * Platform detection for computer control feature.
 */

export type Platform = 'linux' | 'macos' | 'windows';

/**
 * Detect the current operating system platform.
 * Falls back to 'linux' if unknown.
 */
export function detectPlatform(): Platform {
  const os = process.platform;
  if (os === 'linux') return 'linux';
  if (os === 'darwin') return 'macos';
  if (os === 'win32') return 'windows';
  return 'linux'; // fallback
}

/**
 * Check if the current platform is Linux with Wayland.
 */
export function isWayland(): boolean {
  return process.platform === 'linux' && Boolean(process.env.WAYLAND_DISPLAY);
}

/**
 * Check if the current platform is Linux with X11.
 */
export function isX11(): boolean {
  return process.platform === 'linux' && Boolean(process.env.DISPLAY) && !process.env.WAYLAND_DISPLAY;
}

/**
 * Check if running on Hyprland.
 */
export function isHyprland(): boolean {
  return process.platform === 'linux' && Boolean(process.env.HYPRLAND_INSTANCE_SIGNATURE);
}
