/**
 * Image Generation Feature
 *
 * AI image generation with multiple provider support (DALL-E, Stable Diffusion, Midjourney).
 * Provides image caching, upscaling, and variations.
 */

import crypto from 'crypto';
import { mkdirSync, existsSync, writeFileSync, unlinkSync } from 'fs';
import { join } from 'path';

import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';

/** Supported image providers */
export type ImageProvider = 'dalle' | 'stable-diffusion' | 'midjourney';

/** Image generation request */
export interface GenerateRequest {
  prompt: string;
  size?: '256x256' | '512x512' | '1024x1024' | '1920x1080' | '1080x1920';
  style?: 'natural' | 'vivid' | 'anime' | 'realistic' | 'artistic';
  numImages?: number; // 1-10 depending on provider
  negativePrompt?: string; // For SD/MJ
  seed?: number; // For reproducible results
  steps?: number; // For SD
  guidanceScale?: number; // For SD
}

/** Generated image result */
export interface GeneratedImage {
  id: string;
  path: string;
  url?: string; // Remote URL if provider returns one
  prompt: string;
  size: string;
  style?: string;
  provider: ImageProvider;
  createdAt: number;
  metadata?: Record<string, unknown>;
}

/** Feature configuration */
export interface ImageGenerationConfig {
  cacheDir?: string;
  maxCacheSizeMB?: number;
  defaultProvider?: ImageProvider;
  defaultSize?: string;
  apiKeys?: Record<ImageProvider, string>; // Provider API keys
}

/**
 * ImageGeneration — AI-powered image generation with provider abstraction.
 *
 * Provides:
 * - Multiple provider support (DALL-E, Stable Diffusion, Midjourney)
 * - Image caching to disk with content-addressed filenames
 * - Upscaling and variations structure
 * - Generation history tracking
 *
 * Real API integration is not implemented; uses placeholder images.
 */
class ImageGenerationFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'image-generation',
    version: '0.1.0',
    description: 'AI image generation with DALL-E, Stable Diffusion, and Midjourney support',
    dependencies: [],
  };

  private config: Required<ImageGenerationConfig>;
  private ctx!: FeatureContext;
  private provider: ImageProvider = 'dalle';
  private apiKeys: Map<ImageProvider, string> = new Map();
  private cacheDir!: string;
  private cacheIndex: Map<string, GeneratedImage> = new Map();

  constructor() {
    this.config = {
      cacheDir: './data/images',
      maxCacheSizeMB: 10000,
      defaultProvider: 'dalle',
      defaultSize: '1024x1024',
      apiKeys: {} as Record<string, string>,
    };
  }

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = {
      cacheDir: (config['cacheDir'] as string) ?? this.config['cacheDir'],
      maxCacheSizeMB: (config['maxCacheSizeMB'] as number) ?? this.config['maxCacheSizeMB'],
      defaultProvider: (config['defaultProvider'] as ImageProvider) ?? this.config['defaultProvider'],
      defaultSize: (config['defaultSize'] as string) ?? this.config['defaultSize'],
      apiKeys: (config['apiKeys'] as Record<ImageProvider, string>) ?? this.config['apiKeys'],
    };

    this.provider = this.config.defaultProvider;
    this.cacheDir = this.config.cacheDir;

    // Load API keys from config
    if (this.config.apiKeys) {
      for (const [prov, key] of Object.entries(this.config.apiKeys)) {
        this.apiKeys.set(prov as ImageProvider, key);
      }
    }

    this.initCache();
  }

  async start(): Promise<void> {
    this.ctx.logger.info('ImageGeneration active', {
      provider: this.provider,
      cacheDir: this.cacheDir,
      cachedImages: this.cacheIndex.size,
    });
  }

  async stop(): Promise<void> {
    this.ctx.logger.info('ImageGeneration stopped');
  }

  async healthCheck(): Promise<HealthStatus> {
    try {
      const cacheSizeMB = Array.from(this.cacheIndex.values()).reduce((acc, img) => acc + ((img.metadata as { size: number })?.size ?? 0), 0) / (1024 * 1024);
      const imageCount = this.cacheIndex.size;

      return {
        healthy: true,
        details: {
          provider: this.provider,
          cachedImages: imageCount,
          cacheSizeMB: cacheSizeMB.toFixed(2),
        },
      };
    } catch (err) {
      return {
        healthy: false,
        message: err instanceof Error ? err.message : String(err),
      };
    }
  }

  /**
   * Set the default provider.
   */
  setProvider(provider: ImageProvider): void {
    this.provider = provider;
    this.ctx.logger.info('Image provider changed', { provider });
  }

  /**
   * Set an API key for a provider.
   */
  setApiKey(provider: ImageProvider, apiKey: string): void {
    this.apiKeys.set(provider, apiKey);
    this.ctx.logger.info('Image API key set', { provider });
  }

  /**
   * Generate an image from a prompt (stub).
   *
   * @param request - Generation parameters
   * @returns GeneratedImage with local file path
   */
  async generate(request: GenerateRequest): Promise<GeneratedImage> {
    const {
      prompt,
      size = this.config.defaultSize,
      style,
      seed,
    } = request as { prompt: string; size?: string; style?: string; seed?: number };

    if (!prompt.trim()) {
      throw new Error('Prompt cannot be empty');
    }

    const cacheKey = this.generateCacheKey(prompt, size, style, seed, this.provider);

    // Check cache
    const cached = this.cacheIndex.get(cacheKey);
    if (cached && existsSync(cached.path)) {
      this.ctx.logger.debug('Image cache hit', { cacheKey });
      return cached;
    }

    this.ctx.logger.debug('Generating image (stub)', { provider: this.provider, prompt: `${prompt.substring(0, 50)  }...` });

    // In real implementation, would call provider API
    // For stub, create a placeholder text file as "image"
    const id = `img-${Date.now()}-${crypto.randomBytes(8).toString('hex')}`;
    const ext = 'png';
    const filename = `${id}.${ext}`;
    const filepath = join(this.cacheDir, filename);

    // Ensure cache directory exists
    if (!existsSync(this.cacheDir)) {
      mkdirSync(this.cacheDir, { recursive: true });
    }

    // Create a placeholder image file (in real impl, would be binary PNG/JPG)
    const placeholder = `[IMAGE GENERATION STUB]\nProvider: ${this.provider}\nPrompt: ${prompt}\nSize: ${size}\nStyle: ${style ?? 'default'}\n`;
    writeFileSync(filepath, placeholder);

    const stats = require('fs').statSync(filepath);
    const sizeBytes = stats.size;

    const image: GeneratedImage = {
      id,
      path: filepath,
      prompt,
      size,
      style,
      provider: this.provider,
      createdAt: Date.now(),
      metadata: {
        size: sizeBytes,
        format: ext,
        seed,
        cached: false,
      },
    };

    this.cacheIndex.set(cacheKey, image);

    this.ctx.logger.info('Image generated (stub)', { id, provider: image.provider });
    return image;
  }

  /**
   * Upscale an image (stub - would use provider upscaling or img2img).
   *
   * @param imagePath - Path to existing image
   * @param factor - Upscale factor (2x, 4x, etc.)
   * @returns New GeneratedImage with upscaled version
   */
  async upscale(imagePath: string, factor: number = 2): Promise<GeneratedImage> {
    if (!existsSync(imagePath)) {
      throw new Error(`Image not found: ${imagePath}`);
    }

    const id = `upscaled-${Date.now()}-${crypto.randomBytes(8).toString('hex')}`;
    const filename = `${id}.png`;
    const outPath = join(this.cacheDir, filename);

    // In real impl, would process image with upscaling algorithm
    const placeholder = `[UPSCALE STUB] Original: ${imagePath}, Factor: ${factor}x\n`;
    writeFileSync(outPath, placeholder);

    const stats = require('fs').statSync(outPath);

    const image: GeneratedImage = {
      id,
      path: outPath,
      prompt: '',
      size: `${factor}x upscaled`,
      provider: this.provider,
      createdAt: Date.now(),
      metadata: {
        size: stats.size,
        basedOn: imagePath,
        upscaleFactor: factor,
      },
    };

    this.ctx.logger.info('Image upscaled (stub)', { id, factor });
    return image;
  }

  /**
   * Create variations of an image (stub).
   *
   * @param imagePath - Source image
   * @param count - Number of variations to generate (default 3)
   * @returns Array of GeneratedImage variations
   */
  async variations(imagePath: string, count: number = 3): Promise<GeneratedImage[]> {
    if (!existsSync(imagePath)) {
      throw new Error(`Image not found: ${imagePath}`);
    }

    const variations: GeneratedImage[] = [];

    for (let i = 0; i < count; i++) {
      const id = `var-${Date.now()}-${i}-${crypto.randomBytes(4).toString('hex')}`;
      const filename = `${id}.png`;
      const outPath = join(this.cacheDir, filename);

      const placeholder = `[VARIATION STUB] Source: ${imagePath}, Variation: ${i + 1}\n`;
      writeFileSync(outPath, placeholder);

      const stats = require('fs').statSync(outPath);

      variations.push({
        id,
        path: outPath,
        prompt: '',
        size: 'variation',
        provider: this.provider,
        createdAt: Date.now(),
        metadata: {
          size: stats.size,
          basedOn: imagePath,
          variationIndex: i,
        },
      });
    }

    this.ctx.logger.info('Variations created (stub)', { source: imagePath, count });
    return variations;
  }

  /**
   * Delete a generated image.
   */
  deleteImage(imageId: string): boolean {
    const found = Array.from(this.cacheIndex.values()).find(img => img.id === imageId);
    if (!found) return false;

    try {
      if (existsSync(found.path)) {
        unlinkSync(found.path);
      }
      // Remove from cache index
      for (const [key, img] of this.cacheIndex.entries()) {
        if (img.id === imageId) {
          this.cacheIndex.delete(key);
          break;
        }
      }
      this.ctx.logger.info('Image deleted', { imageId });
      return true;
    } catch (err) {
      this.ctx.logger.warn('Failed to delete image', { imageId, error: err instanceof Error ? err.message : String(err) });
      return false;
    }
  }

  /**
   * Get image by ID.
   */
  getImage(imageId: string): GeneratedImage | null {
    return Array.from(this.cacheIndex.values()).find(img => img.id === imageId) ?? null;
  }

  /**
   * List generated images with optional filters.
   */
  listImages(limit: number = 100, provider?: ImageProvider): GeneratedImage[] {
    let images = Array.from(this.cacheIndex.values());

    if (provider) {
      images = images.filter(img => img.provider === provider);
    }

    return images.sort((a, b) => b.createdAt - a.createdAt).slice(0, limit);
  }

  /**
   * Clear old cached images.
   *
   * @param olderThanDays - Delete images older than this many days
   * @returns number of images deleted
   */
  clearCache(olderThanDays: number = 30): number {
    const cutoff = Date.now() - olderThanDays * 24 * 60 * 60 * 1000;
    let deleted = 0;

    for (const [key, image] of this.cacheIndex.entries()) {
      if (image.createdAt < cutoff) {
        try {
          if (existsSync(image.path)) {
            unlinkSync(image.path);
          }
          this.cacheIndex.delete(key);
          deleted++;
        } catch {
          // Ignore
        }
      }
    }

    if (deleted > 0) {
      this.ctx.logger.info('Image cache cleared', { deleted });
    }

    return deleted;
  }

  /**
   * Get cache statistics.
   */
  getCacheStats(): { images: number; sizeMB: number } {
    const size = Array.from(this.cacheIndex.values()).reduce((acc, img) => acc + ((img.metadata as { size: number })?.size ?? 0), 0);
    return {
      images: this.cacheIndex.size,
      sizeMB: size / (1024 * 1024),
    };
  }

  /** Generate a content-addressed cache key */
  private generateCacheKey(prompt: string, size: string, style?: string, seed?: number, provider?: ImageProvider): string {
    const data = `${prompt}:${size}:${style ?? ''}:${seed ?? ''}:${provider ?? this.provider}`;
    return crypto.createHash('sha256').update(data).digest('hex');
  }

  /** Initialize cache directory */
  private initCache(): void {
    if (!existsSync(this.cacheDir)) {
      mkdirSync(this.cacheDir, { recursive: true });
    }

    // Load existing cache files on startup? Could scan directory
    // For now just ensure dir exists
  }
}

export default new ImageGenerationFeature();
