"use strict";
/**
 * Video Processing Feature
 *
 * Video manipulation using ffmpeg. Supports frame extraction, trimming,
 * and metadata retrieval. Gracefully degrades if ffmpeg is not available.
 */
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const child_process_1 = require("child_process");
const fs_1 = require("fs");
const path_1 = require("path");
const util_1 = require("util");
const better_sqlite3_1 = __importDefault(require("better-sqlite3"));
const execFile = (0, util_1.promisify)(child_process_1.execFile);
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
class VideoProcessingFeature {
    meta = {
        name: 'video-processing',
        version: '0.1.0',
        description: 'Video processing with ffmpeg (frame extraction, trimming, metadata)',
        dependencies: [],
    };
    config;
    ctx;
    db;
    ffmpegPath = 'ffmpeg';
    constructor() {
        this.config = {
            ffmpegPath: 'ffmpeg',
            outputDir: './data/video-processing/frames',
            maxConcurrentJobs: 2,
            logDir: './data/video-processing/logs',
        };
    }
    async init(config, context) {
        this.ctx = context;
        this.config = {
            ffmpegPath: config['ffmpegPath'] ?? this.config['ffmpegPath'],
            outputDir: config['outputDir'] ?? this.config['outputDir'],
            maxConcurrentJobs: config['maxConcurrentJobs'] ?? this.config['maxConcurrentJobs'],
            logDir: config['logDir'] ?? this.config['logDir'],
        };
        this.ffmpegPath = this.config.ffmpegPath;
        this.initDirectories();
        this.initDatabase();
        // Verify ffmpeg availability
        await this.verifyFfmpeg();
    }
    async start() {
        this.ctx.logger.info('VideoProcessing active', {
            ffmpeg: this.ffmpegPath,
            outputDir: this.config.outputDir,
        });
    }
    async stop() {
        if (this.db) {
            this.db.close();
            this.ctx.logger.info('VideoProcessing stopped');
        }
    }
    async healthCheck() {
        try {
            const jobsTotal = this.db.prepare('SELECT COUNT(*) as c FROM jobs').get()
                .c;
            const failedJobs = this.db.prepare("SELECT COUNT(*) as c FROM jobs WHERE status = 'failed'").get().c;
            return {
                healthy: true,
                details: {
                    ffmpegPath: this.ffmpegPath,
                    totalJobs: jobsTotal,
                    failedJobs,
                    outputDir: this.config.outputDir,
                },
            };
        }
        catch (err) {
            return {
                healthy: false,
                message: err instanceof Error ? err.message : String(err),
            };
        }
    }
    /**
     * Verify ffmpeg is available.
     */
    async verifyFfmpeg() {
        try {
            const { stdout } = await execFile(this.ffmpegPath, ['-version'], { timeout: 10000 });
            const versionLine = stdout?.split('\n')[0] ?? '';
            this.ctx.logger.info('ffmpeg found', { version: versionLine.trim() });
        }
        catch (err) {
            this.ctx.logger.error('ffmpeg not available', {
                error: err instanceof Error ? err.message : String(err),
            });
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
    async extractFrames(videoPath, options = {}) {
        const { interval = 1, startTime = 0, endTime, outputDir = (0, path_1.join)(this.config.outputDir, (0, path_1.basename)(videoPath, this.ext(videoPath))), format = 'png', quality, } = options;
        if (!(0, fs_1.existsSync)(videoPath)) {
            throw new Error(`Video file not found: ${videoPath}`);
        }
        if (!(0, fs_1.existsSync)(outputDir)) {
            (0, fs_1.mkdirSync)(outputDir, { recursive: true });
        }
        const jobId = `extract-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
        this.logJobStart(jobId, 'extract_frames', videoPath, {
            interval,
            startTime,
            endTime,
            outputDir,
            format,
        });
        try {
            // Build ffmpeg command
            // -i input -vf fps=1/interval output_%04d.png
            // Or using -ss to seek and -t for duration
            let vfFilter = '';
            if (format === 'png') {
                vfFilter = `fps=1/${interval}`;
            }
            else {
                // jpg quality
                vfFilter = `fps=1/${interval}`;
            }
            const args = [
                '-hide_banner',
                '-loglevel',
                'error',
                '-i',
                videoPath,
                '-ss',
                startTime.toString(),
            ];
            if (endTime) {
                args.push('-t', (endTime - startTime).toString());
            }
            args.push('-vf', vfFilter);
            if (format === 'jpg' && quality !== undefined) {
                args.push('-q:v', quality.toString());
            }
            args.push((0, path_1.join)(outputDir, `frame_%04d.${format}`));
            this.ctx.logger.debug('Running ffmpeg', { args });
            await execFile(this.ffmpegPath, args, { timeout: 3600000 }); // 1hr timeout
            // Read output directory to find generated frames
            const files = (0, fs_1.readdirSync)(outputDir)
                .filter((f) => f.startsWith('frame_') && f.endsWith(`.${format}`))
                .sort();
            const framePaths = files.map((f) => (0, path_1.join)(outputDir, f));
            this.logJobComplete(jobId, true, { frameCount: framePaths.length });
            this.ctx.logger.info('Frames extracted', {
                video: videoPath,
                count: framePaths.length,
                outputDir,
            });
            return {
                outputDir,
                frames: framePaths,
                frameCount: framePaths.length,
                interval,
            };
        }
        catch (err) {
            this.logJobComplete(jobId, false, {
                error: err instanceof Error ? err.message : String(err),
            });
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
    async trim(videoPath, startTime, endTime, outputPath) {
        if (!(0, fs_1.existsSync)(videoPath)) {
            throw new Error(`Video file not found: ${videoPath}`);
        }
        const duration = endTime - startTime;
        if (duration <= 0) {
            throw new Error('Invalid trim range: endTime must be greater than startTime');
        }
        const outPath = outputPath ?? (0, path_1.join)((0, path_1.dirname)(videoPath), `trimmed_${(0, path_1.basename)(videoPath)}`);
        const jobId = `trim-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
        this.logJobStart(jobId, 'trim', videoPath, { startTime, endTime, outputPath: outPath });
        try {
            // ffmpeg -i input -ss start -t duration -c copy output
            await execFile(this.ffmpegPath, [
                '-hide_banner',
                '-loglevel',
                'error',
                '-i',
                videoPath,
                '-ss',
                startTime.toString(),
                '-t',
                duration.toString(),
                '-c',
                'copy',
                outPath,
            ], { timeout: 3600000 });
            const stats = require('fs').statSync(outPath);
            const originalStats = require('fs').statSync(videoPath);
            this.logJobComplete(jobId, true, { outputSize: stats.size });
            this.ctx.logger.info('Video trimmed', { input: videoPath, output: outPath, duration });
            return {
                outputPath: outPath,
                duration,
                originalDuration: originalStats.size / 1000000, // rough estimate
            };
        }
        catch (err) {
            this.logJobComplete(jobId, false, {
                error: err instanceof Error ? err.message : String(err),
            });
            throw err;
        }
    }
    /**
     * Get video metadata using ffprobe.
     *
     * @param videoPath - Path to video file
     * @returns VideoMetadata
     */
    async getMetadata(videoPath) {
        if (!(0, fs_1.existsSync)(videoPath)) {
            throw new Error(`Video file not found: ${videoPath}`);
        }
        try {
            // Use ffprobe to get JSON metadata
            const { stdout } = await execFile('ffprobe', [
                '-v',
                'error',
                '-select_streams',
                'v:0',
                '-show_entries',
                'stream=width,height,r_frame_rate,codec_name,avg_bitrate,duration',
                '-of',
                'json',
                videoPath,
            ], { timeout: 30000 });
            const probe = JSON.parse(stdout);
            const stream = probe.streams[0];
            // Parse frame rate (e.g., "30/1" -> 30)
            const fpsParts = stream.r_frame_rate.split('/');
            const fps = fpsParts.length === 2
                ? parseInt(fpsParts[0]) / parseInt(fpsParts[1])
                : parseFloat(stream.r_frame_rate);
            const stats = require('fs').statSync(videoPath);
            const metadata = {
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
            this.ctx.logger.debug('Metadata retrieved', {
                video: videoPath,
                duration: metadata.duration,
            });
            return metadata;
        }
        catch (err) {
            this.ctx.logger.error('Failed to get metadata', {
                video: videoPath,
                error: err instanceof Error ? err.message : String(err),
            });
            throw err;
        }
    }
    /**
     * Convert video to another format (stub - future).
     */
    async convert(_videoPath, _outputFormat) {
        // Future implementation
        throw new Error('Not implemented yet');
    }
    /** Initialize output and log directories */
    initDirectories() {
        if (!(0, fs_1.existsSync)(this.config.outputDir)) {
            (0, fs_1.mkdirSync)(this.config.outputDir, { recursive: true });
        }
        if (!(0, fs_1.existsSync)(this.config.logDir)) {
            (0, fs_1.mkdirSync)(this.config.logDir, { recursive: true });
        }
    }
    /** Initialize database */
    initDatabase() {
        const dbPath = (0, path_1.join)(this.config.logDir, 'jobs.db');
        if (!(0, fs_1.existsSync)((0, path_1.dirname)(dbPath))) {
            (0, fs_1.mkdirSync)((0, path_1.dirname)(dbPath), { recursive: true });
        }
        this.db = new better_sqlite3_1.default(dbPath);
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
    logJobStart(id, type, inputPath, params) {
        this.db
            .prepare(`
      INSERT INTO jobs (id, type, input_path, params, status, started_at)
      VALUES (?, ?, ?, ?, ?, ?)
    `)
            .run(id, type, inputPath, JSON.stringify(params), 'running', Date.now());
    }
    /** Log job completion */
    logJobComplete(id, success, extra) {
        const result = success ? 'completed' : 'failed';
        const resultData = JSON.stringify(extra);
        this.db
            .prepare(`
      UPDATE jobs SET status = ?, completed_at = ?, result = ?
      WHERE id = ?
    `)
            .run(result, Date.now(), resultData, id);
    }
    /** Get file extension */
    ext(path) {
        const parts = path.split('.');
        return parts.length > 1 ? (parts[parts.length - 1] ?? '').toLowerCase() : '';
    }
}
exports.default = new VideoProcessingFeature();
//# sourceMappingURL=index.js.map