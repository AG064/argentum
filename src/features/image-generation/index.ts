/**
 * Image Generation Feature
 *
 * AI image generation via Google's Gemini 3 Pro Image with
 * automatic SiliconFlow FLUX.1-dev fallback.
 *
 * Primary: Gemini Nano Banana Pro (gemini-3-pro-image)
 * Fallback: SiliconFlow FLUX.1-dev
 */

import { spawn } from 'child_process';
import { existsSync } from 'fs';
import { join } from 'path';

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../../core/plugin-loader';

// ─── Types ────────────────────────────────────────────────────────────────────

export type ImageResolution = '1K' | '2K' | '4K';

export interface GenerateImageOptions {
  /** Output filename */
  filename: string;
  /** Resolution: 1K (1024x1024), 2K (2048x2048), 4K (2048x2048 with higher quality) */
  resolution?: ImageResolution;
  /** Optional input image for editing (Gemini only) */
  inputImage?: string;
  /** Override Gemini API key */
  apiKey?: string;
  /** Override SiliconFlow API key */
  fallbackApiKey?: string;
  /** Timeout in ms (default 180000) */
  timeout?: number;
}

export interface GenerateImageResult {
  /** Absolute path to generated image */
  path: string;
  /** Provider used: 'gemini' | 'siliconflow' */
  provider: 'gemini' | 'siliconflow';
  /** Resolution used */
  resolution: ImageResolution;
  /** Raw stdout from the script */
  stdout: string;
  /** Raw stderr from the script */
  stderr: string;
  /** Exit code */
  exitCode: number;
}

// ─── Constants ────────────────────────────────────────────────────────────────

const GENERATE_SCRIPT_PATH = join(
  process.env.HOME || '/home/ag064',
  '.openclaw',
  'workspace',
  'skills',
  'image-gen',
  'scripts',
  'generate_image.py',
);

const DEFAULT_TIMEOUT_MS = 180_000;

// ─── Feature ─────────────────────────────────────────────────────────────────

class ImageGenerationFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'image-generation',
    version: '1.0.0',
    description:
      'AI image generation via Gemini 3 Pro Image with SiliconFlow FLUX.1-dev fallback',
    dependencies: [],
  };

  private ctx!: FeatureContext;
  private scriptPath: string = GENERATE_SCRIPT_PATH;

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;

    if (config['scriptPath'] && typeof config['scriptPath'] === 'string') {
      this.scriptPath = config['scriptPath'] as string;
    }

    if (!existsSync(this.scriptPath)) {
      this.ctx.logger.warn('generate_image.py not found at expected path', {
        path: this.scriptPath,
      });
    }
  }

  async start(): Promise<void> {
    this.ctx.logger.info('ImageGeneration feature started', {
      scriptPath: this.scriptPath,
    });
  }

  async stop(): Promise<void> {
    this.ctx.logger.info('ImageGeneration feature stopped');
  }

  async healthCheck(): Promise<HealthStatus> {
    const scriptExists = existsSync(this.scriptPath);
    return {
      healthy: scriptExists,
      message: scriptExists
        ? `Script found at ${this.scriptPath}`
        : `Script not found at ${this.scriptPath}`,
      details: {
        scriptPath: this.scriptPath,
        scriptExists,
      },
    };
  }

  /**
   * Generate an image using the Python script.
   *
   * Runs: uv run python3 <script> --prompt <prompt> --filename <filename>
   *       [--resolution 1K|2K|4K] [--input-image <path>] [--api-key <key>] [--fallback-api-key <key>]
   *
   * Detects quota errors in stderr and reports them in result.
   */
  async generateImage(
    prompt: string,
    options: GenerateImageOptions,
  ): Promise<GenerateImageResult> {
    return new Promise((resolve, reject) => {
      const {
        filename,
        resolution = '1K',
        inputImage,
        apiKey,
        fallbackApiKey,
        timeout = DEFAULT_TIMEOUT_MS,
      } = options;

      if (!prompt.trim()) {
        reject(new Error('Prompt cannot be empty'));
        return;
      }

      const args = [
        'run',
        'python3',
        this.scriptPath,
        '--prompt',
        prompt,
        '--filename',
        filename,
        '--resolution',
        resolution,
      ];

      if (inputImage) {
        args.push('--input-image', inputImage);
      }

      if (apiKey) {
        args.push('--api-key', apiKey);
      }

      if (fallbackApiKey) {
        args.push('--fallback-api-key', fallbackApiKey);
      }

      this.ctx.logger.debug('Spawning image generation', {
        cmd: 'uv',
        args: args.join(' '),
      });

      let stdout = '';
      let stderr = '';
      let settled = false;

      const proc = spawn('uv', args, {
        env: {
          ...process.env,
          GEMINI_API_KEY: apiKey || process.env.GEMINI_API_KEY || '',
          SILICONFLOW_API_KEY: fallbackApiKey || process.env.SILICONFLOW_API_KEY,
        },
      });

      const timer = setTimeout(() => {
        if (!settled) {
          settled = true;
          proc.kill('SIGKILL');
          reject(new Error(`Image generation timed out after ${timeout}ms`));
        }
      }, timeout);

      proc.stdout?.on('data', (data: Buffer) => {
        stdout += data.toString();
      });

      proc.stderr?.on('data', (data: Buffer) => {
        stderr += data.toString();
      });

      proc.on('close', (code: number | null) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);

        const exitCode = code ?? -1;

        // Determine provider from stderr content
        const usedFallback =
          stderr.includes('[SiliconFlow') ||
          stderr.includes('Falling back to SiliconFlow') ||
          stderr.includes('fallback');

        this.ctx.logger.debug('Image generation complete', {
          exitCode,
          usedFallback,
          stdoutLines: stdout.split('\n').length,
        });

        resolve({
          path: filename,
          provider: usedFallback ? 'siliconflow' : 'gemini',
          resolution,
          stdout,
          stderr,
          exitCode,
        });
      });

      proc.on('error', (err: Error) => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        reject(err);
      });
    });
  }
}

export const imageGeneration = new ImageGenerationFeature();
export default imageGeneration;

// ─── Standalone exports for direct script usage ───────────────────────────────

/**
 * Generate an image directly (for use outside feature context).
 * Calls the generate_image.py script via uv run.
 *
 * @param prompt - Image description
 * @param options - Generation options
 * @returns Promise<GenerateImageResult>
 */
export async function generateImage(
  prompt: string,
  options: GenerateImageOptions,
): Promise<GenerateImageResult> {
  return imageGeneration.generateImage(prompt, options);
}
