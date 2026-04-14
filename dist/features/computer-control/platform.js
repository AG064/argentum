"use strict";
/**
 * Platform detection for computer control feature.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.detectPlatform = detectPlatform;
exports.isWayland = isWayland;
exports.isX11 = isX11;
exports.isHyprland = isHyprland;
/**
 * Detect the current operating system platform.
 * Falls back to 'linux' if unknown.
 */
function detectPlatform() {
    const os = process.platform;
    if (os === 'linux')
        return 'linux';
    if (os === 'darwin')
        return 'macos';
    if (os === 'win32')
        return 'windows';
    return 'linux'; // fallback
}
/**
 * Check if the current platform is Linux with Wayland.
 */
function isWayland() {
    return process.platform === 'linux' && Boolean(process.env.WAYLAND_DISPLAY);
}
/**
 * Check if the current platform is Linux with X11.
 */
function isX11() {
    return process.platform === 'linux' && Boolean(process.env.DISPLAY) && !process.env.WAYLAND_DISPLAY;
}
/**
 * Check if running on Hyprland.
 */
function isHyprland() {
    return process.platform === 'linux' && Boolean(process.env.HYPRLAND_INSTANCE_SIGNATURE);
}
//# sourceMappingURL=platform.js.map