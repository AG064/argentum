/**
 * AG-Claw YouTube Shorts Generator
 *
 * Takes a YouTube URL, downloads video, finds best moments,
 * cuts into vertical shorts with captions.
 *
 * Requirements (installed separately):
 * - yt-dlp: YouTube downloader
 * - ffmpeg: Video processing
 */

import { execSync } from 'child_process';
import { existsSync, mkdirSync } from 'fs';
import { join } from 'path';

import {
  type FeatureModule,
  type FeatureContext,
  type FeatureMeta,
  type HealthStatus,
} from '../../core/plugin-loader';

// ─── Types ───────────────────────────────────────────────────────────────────

export interface ShortsConfig {
  enabled: boolean;
  outputDir: string;
  maxDuration: number;
  quality: '720p' | '1080p';
  platforms: ('telegram' | 'twitter')[];
  voiceover: boolean;
}

export interface ShortSegment {
  start: number;
  end: number;
  caption: string;
}

export interface ShortResult {
  outputPath: string;
  segment: ShortSegment;
  success: boolean;
  error?: string;
}

// ─── Feature ─────────────────────────────────────────────────────────────────

class YouTubeShortsFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'youtube-shorts',
    version: '0.1.0',
    description: 'Generate short vertical videos from YouTube URLs',
    dependencies: [],
  };

  readonly config: ShortsConfig = {
    enabled: false,
    outputDir: './outputs/shorts',
    maxDuration: 60,
    quality: '1080p',
    platforms: [],
    voiceover: false,
  };

  private ctx: FeatureContext | null = null;

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;

    // Merge config
    if (config['outputDir']) this.config.outputDir = config['outputDir'] as string;
    if (config['maxDuration']) this.config.maxDuration = config['maxDuration'] as number;
    if (config['quality']) this.config.quality = config['quality'] as '720p' | '1080p';
    if (config['platforms']) this.config.platforms = config['platforms'] as ('telegram' | 'twitter')[];
    if (config['voiceover'] !== undefined) this.config.voiceover = config['voiceover'] as boolean;

    // Ensure output directory exists
    if (!existsSync(this.config.outputDir)) {
      mkdirSync(this.config.outputDir, { recursive: true });
    }

    // Check tool availability
    this.checkDependencies();
  }

  async start(): Promise<void> {}

  async stop(): Promise<void> {}

  async healthCheck(): Promise<HealthStatus> {
    const ytDlp = this.isToolAvailable('yt-dlp');
    const ffmpeg = this.isToolAvailable('ffmpeg');

    return {
      healthy: ytDlp && ffmpeg,
      message: ytDlp && ffmpeg
        ? 'YouTube Shorts ready'
        : `Missing tools: ${[!ytDlp ? 'yt-dlp' : '', !ffmpeg ? 'ffmpeg' : ''].filter(Boolean).join(', ')}`,
      details: {
        ytDlp,
        ffmpeg,
        outputDir: this.config.outputDir,
        enabled: this.config.enabled,
      },
    };
  }

  /**
   * Check if required tools are available
   */
  private checkDependencies(): void {
    const ytDlp = this.isToolAvailable('yt-dlp');
    const ffmpeg = this.isToolAvailable('ffmpeg');

    if (!ytDlp) {
      this.ctx?.logger?.warn?.('yt-dlp not found - YouTube downloads disabled');
    }
    if (!ffmpeg) {
      this.ctx?.logger?.warn?.('ffmpeg not found - video processing disabled');
    }
  }

  /**
   * Check if a command is available
   */
  private isToolAvailable(cmd: string): boolean {
    try {
      execSync(`which ${cmd}`, { stdio: 'ignore' });
      return true;
    } catch {
      return false;
    }
  }

  /**
   * Download a YouTube video
   */
  async downloadVideo(url: string, outputPath: string): Promise<string> {
    if (!this.isToolAvailable('yt-dlp')) {
      throw new Error('yt-dlp not installed. Install with: pip install yt-dlp');
    }

    const tmpDir = join(this.config.outputDir, 'tmp');
    mkdirSync(tmpDir, { recursive: true });

    const cmd = `yt-dlp -f "bestvideo[height<=${this.config.quality}]" --merge-output-format mp4 -o "${outputPath}" "${url}"`;
    this.ctx?.logger?.info?.('Downloading video', { url, outputPath });

    execSync(cmd, { stdio: 'inherit' });
    return outputPath;
  }

  /**
   * Find best moments in a video (simplified implementation)
   * Real implementation would use scene detection or ML
   */
  async findBestMoments(_videoPath: string, count: number = 3): Promise<ShortSegment[]> {
    // Simplified: returns evenly distributed segments
    // A real implementation would analyze frames for scene changes
    const segments: ShortSegment[] = [];
    for (let i = 0; i < count; i++) {
      segments.push({
        start: i * 30,
        end: (i + 1) * 30,
        caption: `Moment ${i + 1}`,
      });
    }
    return segments;
  }

  /**
   * Generate a single short video segment
   */
  async generateShort(
    videoPath: string,
    segment: ShortSegment,
    outputPath: string
  ): Promise<void> {
    if (!this.isToolAvailable('ffmpeg')) {
      throw new Error('ffmpeg not installed. Install with: apt install ffmpeg');
    }

    const { start, end, caption } = segment;
    const duration = end - start;
    const escaped = caption.replace(/'/g, "'\\''");

    // Crop to 9:16 vertical, add caption
    const cmd = [
      'ffmpeg',
      '-i', `"${videoPath}"`,
      '-ss', String(start),
      '-t', String(duration),
      '-vf', `"crop=ih*9/16:ih,drawtext=text='${escaped}':fontsize=36:fontcolor=white:x=(w-text_w)/2:y=h-60:borderw=2:bordercolor=black"`,
      '-c:a', 'copy',
      `"${outputPath}"`,
      '-y',
    ].join(' ');

    this.ctx?.logger?.info?.('Generating short', { segment, outputPath });
    execSync(cmd, { stdio: 'ignore' });
  }

  /**
   * Process a YouTube URL and generate shorts
   */
  async processVideo(url: string, segmentCount: number = 3): Promise<ShortResult[]> {
    const tmpDir = join(this.config.outputDir, `tmp-${Date.now()}`);
    mkdirSync(tmpDir, { recursive: true });
    const videoPath = join(tmpDir, 'video.mp4');

    try {
      await this.downloadVideo(url, videoPath);
      const moments = await this.findBestMoments(videoPath, segmentCount);

      const results: ShortResult[] = [];
      for (let i = 0; i < moments.length; i++) {
        const segment = moments[i];
        if (!segment) continue;

        const outPath = join(this.config.outputDir, `short_${Date.now()}_${i}.mp4`);
        try {
          await this.generateShort(videoPath, segment, outPath);
          results.push({
            outputPath: outPath,
            segment,
            success: true,
          });
        } catch (err) {
          const error = err instanceof Error ? err.message : String(err);
          this.ctx?.logger?.error?.(`Failed to generate short ${i}`, { error });
          results.push({
            outputPath: outPath,
            segment,
            success: false,
            error,
          });
        }
      }

      return results;
    } finally {
      // Cleanup tmp directory
      try {
        execSync(`rm -rf "${tmpDir}"`, { stdio: 'ignore' });
      } catch {
        // Ignore cleanup errors
      }
    }
  }
}

export const youtubeShorts = new YouTubeShortsFeature();
export default youtubeShorts;
