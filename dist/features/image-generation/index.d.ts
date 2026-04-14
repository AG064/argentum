/**
 * Image Generation Feature
 *
 * AI image generation via Google's Gemini 3 Pro Image with
 * automatic SiliconFlow FLUX.1-dev fallback.
 *
 * Primary: Gemini Nano Banana Pro (gemini-3-pro-image)
 * Fallback: SiliconFlow FLUX.1-dev
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
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
declare class ImageGenerationFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private ctx;
    private scriptPath;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /**
     * Generate an image using the Python script.
     *
     * Runs: uv run python3 <script> --prompt <prompt> --filename <filename>
     *       [--resolution 1K|2K|4K] [--input-image <path>] [--api-key <key>] [--fallback-api-key <key>]
     *
     * Detects quota errors in stderr and reports them in result.
     */
    generateImage(prompt: string, options: GenerateImageOptions): Promise<GenerateImageResult>;
}
export declare const imageGeneration: ImageGenerationFeature;
export default imageGeneration;
/**
 * Generate an image directly (for use outside feature context).
 * Calls the generate_image.py script via uv run.
 *
 * @param prompt - Image description
 * @param options - Generation options
 * @returns Promise<GenerateImageResult>
 */
export declare function generateImage(prompt: string, options: GenerateImageOptions): Promise<GenerateImageResult>;
//# sourceMappingURL=index.d.ts.map