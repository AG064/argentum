"use strict";
/**
 * Image Generation Feature
 *
 * AI image generation via Google's Gemini 3 Pro Image with
 * automatic SiliconFlow FLUX.1-dev fallback.
 *
 * Primary: Gemini Nano Banana Pro (gemini-3-pro-image)
 * Fallback: SiliconFlow FLUX.1-dev
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.imageGeneration = void 0;
exports.generateImage = generateImage;
const child_process_1 = require("child_process");
const fs_1 = require("fs");
const path_1 = require("path");
// ─── Constants ────────────────────────────────────────────────────────────────
const GENERATE_SCRIPT_PATH = (0, path_1.join)(process.env.HOME ?? '/home/ag064', '.openclaw', 'workspace', 'skills', 'image-gen', 'scripts', 'generate_image.py');
const DEFAULT_TIMEOUT_MS = 180_000;
function firstNonEmptySecret(...values) {
    return values.find((value) => value !== undefined && value.length > 0);
}
// ─── Feature ─────────────────────────────────────────────────────────────────
class ImageGenerationFeature {
    meta = {
        name: 'image-generation',
        version: '0.0.4',
        description: 'AI image generation via Gemini 3 Pro Image with SiliconFlow FLUX.1-dev fallback',
        dependencies: [],
    };
    ctx;
    scriptPath = GENERATE_SCRIPT_PATH;
    async init(config, context) {
        this.ctx = context;
        if (config['scriptPath'] && typeof config['scriptPath'] === 'string') {
            this.scriptPath = config['scriptPath'];
        }
        if (!(0, fs_1.existsSync)(this.scriptPath)) {
            this.ctx.logger.warn('generate_image.py not found at expected path', {
                path: this.scriptPath,
            });
        }
    }
    async start() {
        this.ctx.logger.info('ImageGeneration feature started', {
            scriptPath: this.scriptPath,
        });
    }
    async stop() {
        this.ctx.logger.info('ImageGeneration feature stopped');
    }
    async healthCheck() {
        const scriptExists = (0, fs_1.existsSync)(this.scriptPath);
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
    async generateImage(prompt, options) {
        return new Promise((resolve, reject) => {
            const { filename, resolution = '1K', inputImage, apiKey, fallbackApiKey, timeout = DEFAULT_TIMEOUT_MS, } = options;
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
            const geminiApiKey = firstNonEmptySecret(apiKey, process.env.GEMINI_API_KEY);
            const siliconFlowApiKey = firstNonEmptySecret(fallbackApiKey, process.env.SILICONFLOW_API_KEY);
            const proc = (0, child_process_1.spawn)('uv', args, {
                env: {
                    ...process.env,
                    ...(geminiApiKey ? { GEMINI_API_KEY: geminiApiKey } : {}),
                    ...(siliconFlowApiKey ? { SILICONFLOW_API_KEY: siliconFlowApiKey } : {}),
                },
            });
            const timer = setTimeout(() => {
                if (!settled) {
                    settled = true;
                    proc.kill('SIGKILL');
                    reject(new Error(`Image generation timed out after ${timeout}ms`));
                }
            }, timeout);
            proc.stdout?.on('data', (data) => {
                stdout += data.toString();
            });
            proc.stderr?.on('data', (data) => {
                stderr += data.toString();
            });
            proc.on('close', (code) => {
                if (settled)
                    return;
                settled = true;
                clearTimeout(timer);
                const exitCode = code ?? -1;
                // Determine provider from stderr content
                const usedFallback = stderr.includes('[SiliconFlow') ||
                    stderr.includes('Falling back to SiliconFlow') ||
                    stderr.includes('fallback');
                this.ctx.logger.debug('Image generation complete', {
                    exitCode,
                    usedFallback,
                    stdoutLines: stdout.split('\n').length,
                });
                resolve({
                    path: (0, path_1.resolve)(filename),
                    provider: usedFallback ? 'siliconflow' : 'gemini',
                    resolution,
                    stdout,
                    stderr,
                    exitCode,
                });
            });
            proc.on('error', (err) => {
                if (settled)
                    return;
                settled = true;
                clearTimeout(timer);
                reject(err);
            });
        });
    }
}
exports.imageGeneration = new ImageGenerationFeature();
exports.default = exports.imageGeneration;
// ─── Standalone exports for direct script usage ───────────────────────────────
/**
 * Generate an image directly (for use outside feature context).
 * Calls the generate_image.py script via uv run.
 *
 * @param prompt - Image description
 * @param options - Generation options
 * @returns Promise<GenerateImageResult>
 */
async function generateImage(prompt, options) {
    return exports.imageGeneration.generateImage(prompt, options);
}
//# sourceMappingURL=index.js.map