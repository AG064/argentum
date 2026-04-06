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
export class ComputerAgent {
  private computer: ComputerControl;
  private config: ComputerAgentConfig;

  constructor(computer: ComputerControl, config?: Partial<ComputerAgentConfig>) {
    this.computer = computer;
    this.config = {
      maxIterations: config?.maxIterations ?? 20,
      screenshotIntervalMs: config?.screenshotIntervalMs ?? 1000,
      confidenceThreshold: config?.confidenceThreshold ?? 0.5,
    };
  }

  /**
   * Execute a task on the desktop by repeatedly analyzing screenshots
   * and performing actions until completion or max iterations reached.
   *
   * @param prompt - Natural language description of the task
   * @param analyze - Vision function that takes a screenshot and prompt and returns a decision
   * @returns The final result of the task
   */
  async act(
    prompt: string,
    analyze: (screenshot: ScreenshotResult, context: string) => Promise<VisionDecision>,
  ): Promise<{ success: boolean; iterations: number; error?: string }> {
    let iterations = 0;

    while (iterations < this.config.maxIterations) {
      iterations++;

      // 1. Take screenshot
      const screenshot = await this.computer.screenshot();

      // 2. Analyze with vision
      const decision = await analyze(screenshot, prompt);

      // 3. Execute action
      const action = decision.action;

      if (action.type === 'done') {
        return { success: true, iterations };
      }

      if (action.type === 'error') {
        return { success: false, iterations, error: action.message };
      }

      if (action.type === 'click') {
        await this.computer.mouseClick(action.x, action.y, action.button);
      } else if (action.type === 'move') {
        await this.computer.mouseMove(action.x, action.y);
      } else if (action.type === 'type') {
        for (const char of action.text) {
          await this.computer.keyPress(char);
        }
      } else if (action.type === 'key') {
        await this.computer.keyPress(action.key);
      } else if (action.type === 'keycombo') {
        await this.computer.keyCombo(action.keys);
      } else if (action.type === 'scroll') {
        await this.computer.mouseScroll(action.amount);
      }

      // Wait between iterations
      await new Promise((r) => setTimeout(r, this.config.screenshotIntervalMs));
    }

    return { success: false, iterations, error: 'Max iterations reached' };
  }

  /**
   * Take a single screenshot and return it.
   */
  async screenshot(): Promise<ScreenshotResult> {
    return this.computer.screenshot();
  }

  /**
   * Get current mouse position.
   */
  async getMousePosition() {
    return this.computer.getMousePosition();
  }
}
