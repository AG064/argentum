"use strict";
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
Object.defineProperty(exports, "__esModule", { value: true });
exports.youtubeShorts = void 0;
const child_process_1 = require("child_process");
const crypto_1 = require("crypto");
const fs_1 = require("fs");
const os_1 = require("os");
const path_1 = require("path");
const googleapis_1 = require("googleapis");
// ─── Feature ─────────────────────────────────────────────────────────────────
class YouTubeShortsFeature {
    meta = {
        name: 'youtube-shorts',
        version: '0.0.2',
        description: 'Generate short vertical videos from YouTube URLs and publish to YouTube Shorts',
        dependencies: [],
    };
    config = {
        enabled: false,
        outputDir: './outputs/shorts',
        maxDuration: 60,
        quality: '1080p',
        platforms: [],
        voiceover: false,
    };
    ctx = null;
    youtubeClient = null;
    async init(config, context) {
        this.ctx = context;
        // Merge config
        if (config['outputDir'])
            this.config.outputDir = config['outputDir'];
        if (config['maxDuration'])
            this.config.maxDuration = config['maxDuration'];
        if (config['quality'])
            this.config.quality = config['quality'];
        if (config['platforms'])
            this.config.platforms = config['platforms'];
        if (config['voiceover'] !== undefined)
            this.config.voiceover = config['voiceover'];
        if (config['youtubeApiKey'])
            this.config.youtubeApiKey = config['youtubeApiKey'];
        if (config['youtubeAccessToken'])
            this.config.youtubeAccessToken = config['youtubeAccessToken'];
        if (config['youtubeRefreshToken'])
            this.config.youtubeRefreshToken = config['youtubeRefreshToken'];
        if (config['youtubeClientId'])
            this.config.youtubeClientId = config['youtubeClientId'];
        if (config['youtubeClientSecret'])
            this.config.youtubeClientSecret = config['youtubeClientSecret'];
        if (config['youtubeChannelId'])
            this.config.youtubeChannelId = config['youtubeChannelId'];
        // Ensure output directory exists
        if (!(0, fs_1.existsSync)(this.config.outputDir)) {
            (0, fs_1.mkdirSync)(this.config.outputDir, { recursive: true });
        }
        // Initialize YouTube client if platform is enabled
        if (this.config.platforms.includes('youtube')) {
            this.initYouTubeClient();
        }
        // Check tool availability
        this.checkDependencies();
    }
    async start() { }
    async stop() { }
    async healthCheck() {
        const ytDlp = this.isToolAvailable('yt-dlp');
        const ffmpeg = this.isToolAvailable('ffmpeg');
        const hasYouTubeConfig = this.hasYouTubeCredentials();
        const youtubeHealthy = !this.config.platforms.includes('youtube') || hasYouTubeConfig;
        return {
            healthy: ytDlp && ffmpeg && youtubeHealthy,
            message: this.getHealthMessage(ytDlp, ffmpeg, hasYouTubeConfig),
            details: {
                ytDlp,
                ffmpeg,
                outputDir: this.config.outputDir,
                enabled: this.config.enabled,
                platforms: this.config.platforms,
                youtubeConfigured: hasYouTubeConfig,
            },
        };
    }
    getHealthMessage(ytDlp, ffmpeg, hasYouTube) {
        const missingTools = [];
        if (!ytDlp)
            missingTools.push('yt-dlp');
        if (!ffmpeg)
            missingTools.push('ffmpeg');
        const parts = [];
        if (missingTools.length > 0) {
            parts.push(`Missing tools: ${missingTools.join(', ')}`);
        }
        if (this.config.platforms.includes('youtube') && !hasYouTube) {
            parts.push('YouTube not configured (need YOUTUBE_API_KEY or OAuth2 tokens)');
        }
        if (parts.length === 0) {
            return 'YouTube Shorts ready';
        }
        return parts.join('; ');
    }
    /**
     * Check if YouTube credentials are configured
     */
    hasYouTubeCredentials() {
        // API key auth
        if (this.config.youtubeApiKey)
            return true;
        // OAuth2 auth (access token or refresh token + client credentials)
        if (this.config.youtubeAccessToken)
            return true;
        if (this.config.youtubeRefreshToken && this.config.youtubeClientId && this.config.youtubeClientSecret)
            return true;
        return false;
    }
    /**
     * Initialize YouTube API client
     */
    initYouTubeClient() {
        try {
            const youtubeNamespace = googleapis_1.google.youtube_v3;
            if (!youtubeNamespace) {
                this.ctx?.logger?.error?.('YouTube API v3 not available in googleapis');
                return;
            }
            if (this.config.youtubeApiKey) {
                // API Key authentication
                this.youtubeClient = new youtubeNamespace.Youtube({
                    auth: this.config.youtubeApiKey,
                });
                this.ctx?.logger?.info?.('YouTube client initialized with API key');
            }
            else if (this.config.youtubeAccessToken) {
                // OAuth2 with access token
                const oauth2 = new googleapis_1.google.auth.OAuth2();
                oauth2.setCredentials({ access_token: this.config.youtubeAccessToken });
                this.youtubeClient = new youtubeNamespace.Youtube({ auth: oauth2 });
                this.ctx?.logger?.info?.('YouTube client initialized with OAuth2 access token');
            }
            else if (this.config.youtubeRefreshToken && this.config.youtubeClientId && this.config.youtubeClientSecret) {
                // OAuth2 with refresh token - requires getting new access token
                const oauth2Client = new googleapis_1.google.auth.OAuth2(this.config.youtubeClientId, this.config.youtubeClientSecret);
                oauth2Client.setCredentials({ refresh_token: this.config.youtubeRefreshToken });
                this.youtubeClient = new youtubeNamespace.Youtube({ auth: oauth2Client });
                this.ctx?.logger?.info?.('YouTube client initialized with OAuth2 refresh token');
            }
        }
        catch (err) {
            this.ctx?.logger?.error?.('Failed to initialize YouTube client', { error: err });
            this.youtubeClient = null;
        }
    }
    /**
     * Check if required tools are available
     */
    checkDependencies() {
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
    isToolAvailable(cmd) {
        try {
            (0, child_process_1.execFileSync)('which', [cmd], { stdio: 'ignore' });
            return true;
        }
        catch {
            return false;
        }
    }
    /**
     * Download a YouTube video
     */
    async downloadVideo(url, outputPath) {
        if (!this.isToolAvailable('yt-dlp')) {
            throw new Error('yt-dlp not installed. Install with: pip install yt-dlp');
        }
        const tmpDir = (0, path_1.join)(this.config.outputDir, 'tmp');
        (0, fs_1.mkdirSync)(tmpDir, { recursive: true });
        const ytdlpArgs = [
            '-f', `bestvideo[height<=${this.config.quality}]`,
            '--merge-output-format', 'mp4',
            '-o', outputPath,
            url,
        ];
        this.ctx?.logger?.info?.('Downloading video', { url, outputPath });
        (0, child_process_1.execFileSync)('yt-dlp', ytdlpArgs, { stdio: 'inherit' });
        return outputPath;
    }
    /**
     * Find best moments in a video (simplified implementation)
     * Real implementation would use scene detection or ML
     */
    async findBestMoments(_videoPath, count = 3) {
        // Simplified: returns evenly distributed segments
        // A real implementation would analyze frames for scene changes
        const segments = [];
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
    async generateShort(videoPath, segment, outputPath) {
        if (!this.isToolAvailable('ffmpeg')) {
            throw new Error('ffmpeg not installed. Install with: apt install ffmpeg');
        }
        const { start, end, caption } = segment;
        const duration = end - start;
        // Write caption to a temp file and use textfile= to avoid FFmpeg filter injection
        const captionDir = (0, fs_1.mkdtempSync)((0, path_1.join)((0, os_1.tmpdir)(), 'agclaw-caption-'));
        const captionFile = (0, path_1.join)(captionDir, 'caption.txt');
        try {
            (0, fs_1.writeFileSync)(captionFile, caption, 'utf8');
            const drawtext = `crop=ih*9/16:ih,drawtext=textfile=${captionFile}:fontsize=36:fontcolor=white:x=(w-text_w)/2:y=h-60:borderw=2:bordercolor=black`;
            const ffmpegArgs = [
                '-i', videoPath,
                '-ss', String(start),
                '-t', String(duration),
                '-vf', drawtext,
                '-c:a', 'copy',
                outputPath,
                '-y',
            ];
            this.ctx?.logger?.info?.('Generating short', { segment, outputPath });
            (0, child_process_1.execFileSync)('ffmpeg', ffmpegArgs, { stdio: 'ignore' });
        }
        finally {
            (0, fs_1.rmSync)(captionDir, { recursive: true, force: true });
        }
    }
    /**
     * Upload a video file to YouTube Shorts using googleapis
     */
    async uploadToYouTube(filePath, options = {}) {
        if (!this.youtubeClient) {
            throw new Error('YouTube client not initialized. Configure YOUTUBE_API_KEY or OAuth2 tokens.');
        }
        if (!(0, fs_1.existsSync)(filePath)) {
            throw new Error(`Video file not found: ${filePath}`);
        }
        const { title = `Short ${new Date().toISOString()}`, description = 'Generated with Argentum YouTube Shorts', tags = [], privacyStatus = 'private', } = options;
        // Read video file
        const videoData = (0, fs_1.readFileSync)(filePath);
        // Create a hash for request ID
        const requestId = (0, crypto_1.createHash)('sha256')
            .update(videoData)
            .digest('hex')
            .slice(0, 16);
        this.ctx?.logger?.info?.('Uploading to YouTube Shorts', {
            filePath,
            title,
            privacyStatus,
            requestId,
        });
        try {
            const response = await this.youtubeClient.videos.insert({
                part: ['snippet', 'status'],
                requestBody: {
                    snippet: {
                        title,
                        description,
                        tags,
                        categoryId: '22', // People & Blogs - commonly used for Shorts
                    },
                    status: {
                        privacyStatus,
                        selfDeclaredMadeForKids: false,
                    },
                },
                media: {
                    body: videoData,
                },
            });
            const videoId = response.data.id;
            if (!videoId) {
                throw new Error('YouTube returned no video ID');
            }
            const videoUrl = `https://youtube.com/shorts/${videoId}`;
            this.ctx?.logger?.info?.('YouTube upload successful', { videoId, videoUrl });
            return { videoId, videoUrl };
        }
        catch (err) {
            const errorMessage = this.extractErrorMessage(err);
            this.ctx?.logger?.error?.('YouTube upload failed', { error: errorMessage });
            // Handle specific YouTube API errors
            if (errorMessage.includes('quotaExceeded')) {
                throw new Error('YouTube API quota exceeded. Try again tomorrow.');
            }
            if (errorMessage.includes('unauthorized')) {
                throw new Error('YouTube authorization failed. Check your API key or OAuth token.');
            }
            throw new Error(`YouTube upload failed: ${errorMessage}`);
        }
    }
    /**
     * Extract error message from unknown error type
     */
    extractErrorMessage(err) {
        if (err instanceof Error)
            return err.message;
        if (typeof err === 'string')
            return err;
        return String(err);
    }
    /**
     * Process a YouTube URL and generate shorts
     */
    async processVideo(url, segmentCount = 3, uploadToPlatforms = true) {
        const tmpDir = (0, path_1.join)(this.config.outputDir, `tmp-${Date.now()}`);
        (0, fs_1.mkdirSync)(tmpDir, { recursive: true });
        const videoPath = (0, path_1.join)(tmpDir, 'video.mp4');
        try {
            await this.downloadVideo(url, videoPath);
            const moments = await this.findBestMoments(videoPath, segmentCount);
            const results = [];
            for (let i = 0; i < moments.length; i++) {
                const segment = moments[i];
                if (!segment)
                    continue;
                const outPath = (0, path_1.join)(this.config.outputDir, `short_${Date.now()}_${i}.mp4`);
                try {
                    await this.generateShort(videoPath, segment, outPath);
                    const result = {
                        outputPath: outPath,
                        segment,
                        success: true,
                    };
                    // Upload to YouTube if configured
                    if (uploadToPlatforms && this.config.platforms.includes('youtube') && this.youtubeClient) {
                        try {
                            const { videoId, videoUrl } = await this.uploadToYouTube(outPath, {
                                title: `${segment.caption} #shorts`,
                                description: `Generated short video: ${segment.caption}\n\nCreated with Argentum`,
                                tags: ['shorts', 'ag-claw', 'generated'],
                                privacyStatus: 'public',
                            });
                            result.youtubeVideoId = videoId;
                            result.youtubeUrl = videoUrl;
                        }
                        catch (uploadErr) {
                            const uploadError = this.extractErrorMessage(uploadErr);
                            result.error = `Upload failed: ${uploadError}`;
                            this.ctx?.logger?.error?.(`YouTube upload failed for segment ${i}`, { error: uploadError });
                        }
                    }
                    results.push(result);
                }
                catch (err) {
                    const error = this.extractErrorMessage(err);
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
        }
        finally {
            // Cleanup tmp directory
            try {
                (0, fs_1.rmSync)(tmpDir, { recursive: true, force: true });
            }
            catch {
                // Ignore cleanup errors
            }
        }
    }
}
exports.youtubeShorts = new YouTubeShortsFeature();
exports.default = exports.youtubeShorts;
//# sourceMappingURL=index.js.map