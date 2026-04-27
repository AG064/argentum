/**
 * Argentum YouTube Shorts Generator
 *
 * Takes a YouTube URL, downloads video, finds best moments,
 * cuts into vertical shorts with captions, and optionally
 * publishes to YouTube Shorts.
 *
 * Requirements (installed separately):
 * - yt-dlp: YouTube downloader
 * - ffmpeg: Video processing
 *
 * YouTube Upload Requirements:
 * - YOUTUBE_API_KEY (for API key auth) OR
 * - YOUTUBE_ACCESS_TOKEN (OAuth2 access token)
 */
import { type FeatureModule, type FeatureContext, type FeatureMeta, type HealthStatus } from '../../core/plugin-loader';
export interface ShortsConfig {
    enabled: boolean;
    outputDir: string;
    maxDuration: number;
    quality: '720p' | '1080p';
    platforms: ('telegram' | 'twitter' | 'youtube')[];
    voiceover: boolean;
    youtubeApiKey?: string;
    youtubeAccessToken?: string;
    youtubeRefreshToken?: string;
    youtubeClientId?: string;
    youtubeClientSecret?: string;
    youtubeChannelId?: string;
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
    youtubeVideoId?: string;
    youtubeUrl?: string;
    error?: string;
}
export interface UploadOptions {
    title?: string;
    description?: string;
    tags?: string[];
    privacyStatus?: 'private' | 'unlisted' | 'public';
}
declare class YouTubeShortsFeature implements FeatureModule {
    readonly meta: FeatureMeta;
    readonly config: ShortsConfig;
    private ctx;
    private youtubeClient;
    init(config: Record<string, unknown>, context: FeatureContext): Promise<void>;
    start(): Promise<void>;
    stop(): Promise<void>;
    healthCheck(): Promise<HealthStatus>;
    private getHealthMessage;
    /**
     * Check if YouTube credentials are configured
     */
    private hasYouTubeCredentials;
    /**
     * Initialize YouTube API client
     */
    private initYouTubeClient;
    /**
     * Check if required tools are available
     */
    private checkDependencies;
    /**
     * Check if a command is available
     */
    private isToolAvailable;
    /**
     * Download a YouTube video
     */
    downloadVideo(url: string, outputPath: string): Promise<string>;
    /**
     * Find best moments in a video (simplified implementation)
     * Real implementation would use scene detection or ML
     */
    findBestMoments(_videoPath: string, count?: number): Promise<ShortSegment[]>;
    /**
     * Generate a single short video segment
     */
    generateShort(videoPath: string, segment: ShortSegment, outputPath: string): Promise<void>;
    /**
     * Upload a video file to YouTube Shorts using googleapis
     */
    uploadToYouTube(filePath: string, options?: UploadOptions): Promise<{
        videoId: string;
        videoUrl: string;
    }>;
    /**
     * Extract error message from unknown error type
     */
    private extractErrorMessage;
    /**
     * Process a YouTube URL and generate shorts
     */
    processVideo(url: string, segmentCount?: number, uploadToPlatforms?: boolean): Promise<ShortResult[]>;
}
export declare const youtubeShorts: YouTubeShortsFeature;
export default youtubeShorts;
//# sourceMappingURL=index.d.ts.map