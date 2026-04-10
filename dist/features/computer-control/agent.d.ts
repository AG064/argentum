/**
 * Computer Agent — Vision-based desktop automation loop.
 *
 * Takes screenshots, sends them to an LLM for analysis,
 * and executes the decided actions on the desktop.
 */
import type { ComputerControl, ScreenshotResult, VisionDecision } from './index';
export interface ComputerAgentConfig {
    maxIterations: number;
    screenshotIntervalMs: number;
    confidenceThreshold: number;
}
/**
 * Computer Agent — coordinates vision analysis with desktop control.
 *
 * The agent loop:
 * 1. Take screenshot
 * 2. Send to LLM with user prompt
 * 3. Execute the decided action
 * 4. Repeat until task is complete
 */
export declare class ComputerAgent {
    private computer;
    private config;
    constructor(computer: ComputerControl, config?: Partial<ComputerAgentConfig>);
    /**
     * Execute a task on the desktop by repeatedly analyzing screenshots
     * and performing actions until completion or max iterations reached.
     *
     * @param prompt - Natural language description of the task
     * @param analyze - Vision function that takes a screenshot and prompt and returns a decision
     * @returns The final result of the task
     */
    act(prompt: string, analyze: (screenshot: ScreenshotResult, context: string) => Promise<VisionDecision>): Promise<{
        success: boolean;
        iterations: number;
        error?: string;
    }>;
    /**
     * Take a single screenshot and return it.
     */
    screenshot(): Promise<ScreenshotResult>;
    /**
     * Get current mouse position.
     */
    getMousePosition(): Promise<import("./index").MousePosition>;
}
//# sourceMappingURL=agent.d.ts.map