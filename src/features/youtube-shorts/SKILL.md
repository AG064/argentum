# YouTube Shorts Generator

Auto-generates short vertical videos from YouTube content.

## Setup

Requirements (installed separately):
- `yt-dlp` - YouTube downloader (`pip install yt-dlp`)
- `ffmpeg` - Video processing (`apt install ffmpeg`)

## Usage

```typescript
import { youtubeShorts } from './features/youtube-shorts';

// Check health first
const health = await youtubeShorts.healthCheck();
if (!health.healthy) {
  console.log('Missing:', health.message);
  return;
}

// Process a YouTube video into shorts
const results = await youtubeShorts.processVideo(
  'https://www.youtube.com/watch?v=...',
  3 // number of segments
);

for (const result of results) {
  if (result.success) {
    console.log('Created:', result.outputPath);
  } else {
    console.error('Failed:', result.error);
  }
}
```

## Configuration

```typescript
const config = {
  enabled: true,
  outputDir: './outputs/shorts',
  maxDuration: 60,      // max seconds per short
  quality: '1080p',      // or '720p'
  platforms: ['telegram', 'twitter'],
  voiceover: false,
};
```

## Workflow

1. Download full YouTube video
2. Analyze frames to find best moments (scene changes)
3. Cut 9:16 vertical clips
4. Add captions
5. Publish to configured platforms

## Limitations

- Requires yt-dlp and ffmpeg installed
- YouTube ToS may prohibit downloading
- Feature is disabled by default
- Scene detection is simplified (evenly distributed segments)
