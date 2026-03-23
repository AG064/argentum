/**
 * Video Processing Feature
 *
 * Video manipulation using ffmpeg. Supports frame extraction, trimming,
 * and metadata retrieval. Gracefully degrades if ffmpeg is not available.
 */

import { mkdirSync, existsSync, unlink, readdirSync } from 'fs';
import { join, basename, dirname } from 'path';
import { exec as execOriginal } from 'child_process';
import { promisify } from 'util';
import Database from 'better-sqlite3';
import { FeatureModule, FeatureContext, FeatureMeta, HealthStatus } from '../../core/plugin-loader';

const exec = promisify(execOriginal);

/** Video metadata */
export interface VideoMetadata {
  path: string;
  format: string;
  duration: number; // seconds
  width: number;
  height: number;
  fps?: number;
  codec: string;
  bitrate?: number; // kbps
  size: number; // bytes
  createdAt: number;
}

/** Frame extraction options */
export interface ExtractFramesOptions {
  interval?: number; // Extract one frame every N seconds (default: 1)
  startTime?: number; // Start time in seconds
  endTime?: number; // End time in seconds
  outputDir?: string;
  format?: 'png' | 'jpg';
  quality?: number; // 1-31 for jpg, lower is better
}

/** Frame extraction result */
export interface ExtractFramesResult {
  outputDir: string;
  frames: string[]; // paths to extracted frames
  frameCount: number;
  interval: number;
}

/** Trim operation result */
export interface TrimResult {
  outputPath: string;
  duration: number;
  originalDuration: number;
}

/** Feature configuration */
export interface VideoProcessingConfig {
  ffmpegPath?: string;
  outputDir?: string;
  maxConcurrentJobs?: number;
  logDir?: string;
}

/**
 * VideoProcessing — video manipulation with ffmpeg.
 *
 * Provides:
 * - Frame extraction at intervals
 * - Video trimming
 * - Metadata extraction
 * - Processing logs in SQLite
 *
 * If ffmpeg is not available, operations will fail with clear errors.
 */
class VideoProcessingFeature implements FeatureModule {
  readonly meta: FeatureMeta = {
    name: 'video-processing',
    version: '0.1.0',
    description: 'Video processing with ffmpeg (frame extraction, trimming, metadata)',
    dependencies: [],
  };

  private config: Required<VideoProcessingConfig>;
  private ctx!: FeatureContext;
  private db!: Database.Database;
  private ffmpegPath: string = 'ffmpeg';

  constructor() {
    this.config = {
      ffmpegPath: 'ffmpeg',
      outputDir: './data/video-processing/frames',
      maxConcurrentJobs: 2,
      logDir: './data/video-processing/logs',
    };
  }

  async init(config: Record<string, unknown>, context: FeatureContext): Promise<void> {
    this.ctx = context;
    this.config = {
      ffmpegPath: (config['ffmpegPath'] as string) ?? this.config['ffmpegPath'],
      outputDir: (config['outputDir'] as string) ?? this.config['outputDir'],
      maxConcurrentJobs: (config['maxConcurrentJobs'] as number) ?? this.config['maxConcurrentJobs'],
      logDir: (config['logDir'] as string) ?? this.config['logDir'],
    };

    this.ffmpegPath = this.config.ffmpegPath;
    this.initDirectories();
    this.initDatabase();

    // Verify ffmpeg availability
    this.verifyFfmpeg();
  }

  async start(): Promise<void> {
    this.ctx.logger.info('VideoProcessing active', {
      ffmpeg: this.ffmpegPath,
      outputDir: this.config.outputDir,
    });
  }

  async stop(): Promise<void> {
    if (this.db) {
      this.db.close();
      this.ctx.logger.info('VideoProcessing stopped');
    }
  }

  async healthCheck(): Promise<HealthStatus> {
    try {
      const jobsTotal = (this.db.prepare('SELECT COUNT(*) as c FROM jobs').get() as { c: number }).c;
      const failedJobs = (this.db.prepare("SELECT COUNT(*) as c FROM jobs WHERE status = 'failed'").get() as { c: number }).c;

      return {
        healthy: true,
        details: {
          ffmpegPath: this.ffmpegPath,
          totalJobs: jobsTotal,
          failedJobs,
          outputDir: this.config.outputDir,
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
   * Verify ffmpeg is available.
   */
  private async verifyFfmpeg(): Promise<void> {
    try {
      const { stdout } = await exec(`${this.ffmpegPath} -version`, { timeout: 10000 });
      const versionLine = stdout?.split('\n')[0] ?? '';
      this.ctx.logger.info('ffmpeg found', { version: versionLine.trim() });
    } catch (err) {
      this.ctx.logger.error('ffmpeg not available', { error: err instanceof Error ? err.message : String(err) });
      throw new Error(`ffmpeg not found at ${this.ffmpegPath}. Install ffmpeg to use video processing.`);
    }
  }

  /**
   * Extract frames from a video.
   *
   * @param videoPath - Path to source video
   * @param options - Extraction options
   * @returns ExtractFramesResult with paths to extracted frames
   */
  async extractFrames(videoPath: string, options: ExtractFramesOptions = {}): Promise<ExtractFramesResult> {
    const {
      interval = 1,
      startTime = 0,
      endTime,
      outputDir = join(this.config.outputDir, basename(videoPath, this.ext(videoPath))),
      format = 'png',
      quality,
    } = options;

    if (!existsSync(videoPath)) {
      throw new Error(`Video file not found: ${videoPath}`);
    }

    if (!existsSync(outputDir)) {
      mkdirSync(outputDir, { recursive: true });
    }

    const jobId = `extract-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
    this.logJobStart(jobId, 'extract_frames', videoPath, { interval, startTime, endTime, outputDir, format });

    try {
      // Build ffmpeg command
      // -i input -vf fps=1/interval output_%04d.png
      // Or using -ss to seek and -t for duration
      let vfFilter = '';
      if (format === 'png') {
        vfFilter = `fps=1/${interval}`;
      } else {
        // jpg quality
        vfFilter = `fps=1/${interval}`;
      }

      const args: string[] = [
        '-i', videoPath,
        '-ss', startTime.toString(),
      ];

      if (endTime) {
        args.push('-t', (endTime - startTime).toString());
      }

      args.push('-vf', vfFilter);

      if (format === 'jpg' && quality !== undefined) {
        args.push('-q:v', quality.toString());
      }

      args.push(join(outputDir, `frame_%04d.${format}`));

      const cmd = `${this.ffmpegPath} ${args.join(' ')} -hide_banner -loglevel error`;
      this.ctx.logger.debug('Running ffmpeg', { cmd });

      await exec(cmd, { timeout: 3600000 }); // 1hr timeout

      // Read output directory to find generated frames
      const files = readdirSync(outputDir).filter((f: string) => f.startsWith('frame_') && f.endsWith(`.${format}`)).sort();
      const framePaths = files.map((f: string) => join(outputDir, f));

      this.logJobComplete(jobId, true, { frameCount: framePaths.length });

      this.ctx.logger.info('Frames extracted', { video: videoPath, count: framePaths.length, outputDir });

      return {
        outputDir,
        frames: framePaths,
        frameCount: framePaths.length,
        interval,
      };
    } catch (err) {
      this.logJobComplete(jobId, false, { error: err instanceof Error ? err.message : String(err) });
      throw err;
    }
  }

  /**
   * Trim a video.
   *
   * @param videoPath - Source video
   * @param startTime - Start time in seconds
   * @param endTime - End time in seconds
   * @param outputPath - Destination (optional, default to trimmed_<filename>)
   * @returns TrimResult with output info
   */
  async trim(videoPath: string, startTime: number, endTime: number, outputPath?: string): Promise<TrimResult> {
    if (!existsSync(videoPath)) {
      throw new Error(`Video file not found: ${videoPath}`);
    }

    const duration = endTime - startTime;
    if (duration <= 0) {
      throw new Error('Invalid trim range: endTime must be greater than startTime');
    }

    const outPath = outputPath ?? join(dirname(videoPath), `trimmed_${basename(videoPath)}`);
    const jobId = `trim-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
    this.logJobStart(jobId, 'trim', videoPath, { startTime, endTime, outputPath: outPath });

    try {
      // ffmpeg -i input -ss start -t duration -c copy output
      const cmd = `${this.ffmpegPath} -i ${this.quote(videoPath)} -ss ${startTime} -t ${duration} -c copy ${this.quote(outPath)} -hide_banner -loglevel error`;
      await exec(cmd, { timeout: 3600000 });

      const stats = require('fs').statSync(outPath);
      const originalStats = require('fs').statSync(videoPath);

      this.logJobComplete(jobId, true, { outputSize: stats.size });
      this.ctx.logger.info('Video trimmed', { input: videoPath, output: outPath, duration });

      return {
        outputPath: outPath,
        duration,
        originalDuration: originalStats.size / 1000000, // rough estimate
      };
    } catch (err) {
      this.logJobComplete(jobId, false, { error: err instanceof Error ? err.message : String(err) });
      throw err;
    }
  }

  /**
   * Get video metadata using ffprobe.
   *
   * @param videoPath - Path to video file
   * @returns VideoMetadata
   */
  async getMetadata(videoPath: string): Promise<VideoMetadata> {
    if (!existsSync(videoPath)) {
      throw new Error(`Video file not found: ${videoPath}`);
    }

    try {
      // Use ffprobe to get JSON metadata
      const cmd = `ffprobe -v error -select_streams v:0 -show_entries stream=width,height,r_frame_rate,codec_name,avg_bitrate -of json ${this.quote(videoPath)}`;
      const { stdout } = await exec(cmd, { timeout: 30000 });

      const probe = JSON.parse(stdout);
      const stream = probe.streams[0];

      // Parse frame rate (e.g., "30/1" -> 30)
      const fpsParts = stream.r_frame_rate.split('/');
      const fps = fpsParts.length === 2 ? parseInt(fpsParts[0]) / parseInt(fpsParts[1]) : parseFloat(stream.r_frame_rate);

      const stats = require('fs').statSync(videoPath);

      const metadata: VideoMetadata = {
        path: videoPath,
        format: this.ext(videoPath),
        duration: parseFloat(stream.duration ?? '0'),
        width: stream.width,
        height: stream.height,
        fps,
        codec: stream.codec_name,
        bitrate: stream.avg_bitrate ? parseInt(stream.avg_bitrate) / 1000 : undefined,
        size: stats.size,
        createdAt: stats.mtimeMs,
      };

      this.ctx.logger.debug('Metadata retrieved', { video: videoPath, duration: metadata.duration });
      return metadata;
    } catch (err) {
      this.ctx.logger.error('Failed to get metadata', { video: videoPath, error: err instanceof Error ? err.message : String(err) });
      throw err;
    }
  }

  /**
   * Convert video to another format (stub - future).
   */
  async convert(_videoPath: string, _outputFormat: 'mp4' | 'webm' | 'mov'): Promise<string> {
    // Future implementation
    throw new Error('Not implemented yet');
  }

  /** Initialize output and log directories */
  private initDirectories(): void {
    if (!existsSync(this.config.outputDir)) {
      mkdirSync(this.config.outputDir, { recursive: true });
    }
    if (!existsSync(this.config.logDir)) {
      mkdirSync(this.config.logDir, { recursive: true });
    }
  }

  /** Initialize database */
  private initDatabase(): void {
    const dbPath = join(this.config.logDir, 'jobs.db');
    if (!existsSync(dirname(dbPath))) {
      mkdirSync(dirname(dbPath), { recursive: true });
    }

    this.db = new Database(dbPath);
    this.db.pragma('journal_mode = WAL');
    this.db.pragma('synchronous = NORMAL');

    this.db.exec(`
      CREATE TABLE IF NOT EXISTS jobs (
        id TEXT PRIMARY KEY,
        type TEXT NOT NULL,
        input_path TEXT NOT NULL,
        params TEXT,
        status TEXT NOT NULL,
        started_at INTEGER NOT NULL,
        completed_at INTEGER,
        result TEXT,
        error TEXT
      );

      CREATE INDEX IF NOT EXISTS idx_jobs_started ON jobs(started_at DESC);
      CREATE INDEX IF NOT EXISTS idx_jobs_status ON jobs(status);
    `);
  }

  /** Log job start */
  private logJobStart(id: string, type: string, inputPath: string, params: Record<string, unknown>): void {
    this.db.prepare(`
      INSERT INTO jobs (id, type, input_path, params, status, started_at)
      VALUES (?, ?, ?, ?, ?, ?)
    `).run(id, type, inputPath, JSON.stringify(params), 'running', Date.now());
  }

  /** Log job completion */
  private logJobComplete(id: string, success: boolean, extra: Record<string, unknown>): void {
    const result = success ? 'completed' : 'failed';
    const resultData = JSON.stringify(extra);
    this.db.prepare(`
      UPDATE jobs SET status = ?, completed_at = ?, result = ?
      WHERE id = ?
    `).run(result, Date.now(), resultData, id);
  }

  /** Quote a path for shell */
  private quote(path: string): string {
    // Simple quoting for POSIX shells
    if (path.includes(' ') || path.includes('(') || path.includes(')')) {
      return `'${path.replace(/'/g, `'\\''`)}'`;
    }
    return path;
  }

  /** Get file extension */
  private ext(path: string): string {
    const parts = path.split('.');
    return parts.length > 1 ? (parts[parts.length - 1] ?? '').toLowerCase() : '';
  }
}

export default new VideoProcessingFeature();
