/**
 * Video Processing Feature
 *
 * Video manipulation using ffmpeg. Supports frame extraction, trimming,
 * and metadata retrieval. Gracefully degrades if ffmpeg is not available.
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
/** Video metadata */
export interface VideoMetadata {
    path: string;
    format: string;
    duration: number;
    width: number;
    height: number;
    fps?: number;
    codec: string;
    bitrate?: number;
    size: number;
    createdAt: number;
}
/** Frame extraction options */
export interface ExtractFramesOptions {
    interval?: number;
    startTime?: number;
    endTime?: number;
    outputDir?: string;
    format?: 'png' | 'jpg';
    quality?: number;
}
/** Frame extraction result */
export interface ExtractFramesResult {
    outputDir: string;
    frames: string[];
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
declare class VideoProcessingFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    private config;
    private ctx;
    private db;
    private ffmpegPath;
    constructor();
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    /**
     * Verify ffmpeg is available.
     */
    private verifyFfmpeg;
    /**
     * Extract frames from a video.
     *
     * @param videoPath - Path to source video
     * @param options - Extraction options
     * @returns ExtractFramesResult with paths to extracted frames
     */
    extractFrames(videoPath: string, options?: ExtractFramesOptions): Promise<ExtractFramesResult>;
    /**
     * Trim a video.
     *
     * @param videoPath - Source video
     * @param startTime - Start time in seconds
     * @param endTime - End time in seconds
     * @param outputPath - Destination (optional, default to trimmed_<filename>)
     * @returns TrimResult with output info
     */
    trim(videoPath: string, startTime: number, endTime: number, outputPath?: string): Promise<TrimResult>;
    /**
     * Get video metadata using ffprobe.
     *
     * @param videoPath - Path to video file
     * @returns VideoMetadata
     */
    getMetadata(videoPath: string): Promise<VideoMetadata>;
    /**
     * Convert video to another format (stub - future).
     */
    convert(_videoPath: string, _outputFormat: 'mp4' | 'webm' | 'mov'): Promise<string>;
    /** Initialize output and log directories */
    private initDirectories;
    /** Initialize database */
    private initDatabase;
    /** Log job start */
    private logJobStart;
    /** Log job completion */
    private logJobComplete;
    /** Get file extension */
    private ext;
}
declare const _default: VideoProcessingFeature;
export default _default;
//# sourceMappingURL=index.d.ts.map