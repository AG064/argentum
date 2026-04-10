/**
 * Platform detection for computer control feature.
 */
export type Platform = 'linux' | 'macos' | 'windows';
/**
 * Detect the current operating system platform.
 * Falls back to 'linux' if unknown.
 */
export declare function detectPlatform(): Platform;
/**
 * Check if the current platform is Linux with Wayland.
 */
export declare function isWayland(): boolean;
/**
 * Check if the current platform is Linux with X11.
 */
export declare function isX11(): boolean;
/**
 * Check if running on Hyprland.
 */
export declare function isHyprland(): boolean;
//# sourceMappingURL=platform.d.ts.map