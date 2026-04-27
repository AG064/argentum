/**
 * Argentum Computer Control Feature
 *
 * Vision-based desktop automation through screenshot analysis
 * and mouse/keyboard control. Works on Linux (X11/Wayland/Hyprland),
 * macOS, and Windows.
 *
 * Disabled by default — enable only in safe environments.
 */
import { type Platform } from './platform';
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
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
    data: string;
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
export type VisionAction = {
    type: 'click';
    x: number;
    y: number;
    button?: 'left' | 'right' | 'middle';
} | {
    type: 'move';
    x: number;
    y: number;
} | {
    type: 'type';
    text: string;
} | {
    type: 'key';
    key: string;
} | {
    type: 'keycombo';
    keys: string[];
} | {
    type: 'scroll';
    amount: number;
} | {
    type: 'done';
} | {
    type: 'error';
    message: string;
};
export interface VisionDecision {
    action: VisionAction;
    reason: string;
}
export interface VisionProvider {
    analyze(screenshot: ScreenshotResult, prompt: string): Promise<VisionDecision>;
}
declare class ComputerControlFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private computer;
    private log;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    getComputer(): ComputerControl;
    isEnabled(): boolean;
}
declare const _default: ComputerControlFeature;
export default _default;
export type { ComputerControlFeature };
//# sourceMappingURL=index.d.ts.map