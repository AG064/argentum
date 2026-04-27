# Argentum Mobile Companion App

## Overview

A cross-platform mobile app for Argentum, providing push notifications, camera access, GPS, screen recording, and direct communication with the Argentum gateway.

**Status:** Planning / In Development

## Technology Choice

### Recommended: React Native + Expo

| Criteria | React Native + Expo | Flutter |
|---|---|---|
| Language | TypeScript (shared with Argentum) | Dart (new language to learn) |
| Ecosystem | Massive npm ecosystem | Smaller package ecosystem |
| OTA Updates | Expo EAS Update (instant) | Shorebird (limited) |
| Dev Speed | Fast with Expo Go | Fast hot reload |
| Argentum Integration | Direct TS code sharing | Requires bridging |
| Community | Larger | Growing |

**Decision: React Native + Expo**

TypeScript means code sharing with Argentum core (API types, WebSocket protocols, config schemas). Expo simplifies build/deploy and provides OTA updates without App Store review for JS changes.

## Architecture

```
┌─────────────────────────────────────────────────┐
│              Argentum Mobile App                  │
│                                                  │
│  ┌──────────────┐  ┌──────────────────────────┐ │
│  │  Navigation  │  │     State Management     │ │
│  │  (Expo Router)│  │   (Zustand + React Query)│ │
│  └──────┬───────┘  └──────────┬───────────────┘ │
│         │                      │                  │
│  ┌──────▼──────────────────────▼───────────────┐ │
│  │              Screens                         │ │
│  ├──────────┬──────────┬──────────┬────────────┤ │
│  │  Chat    │ Camera   │  Map     │  Settings  │ │
│  │  (WS)    │ (Vision) │  (GPS)   │            │ │
│  ├──────────┼──────────┼──────────┼────────────┤ │
│  │  Voice   │ Screen   │  Files   │  Status    │ │
│  │ Recorder │ Capture  │  Browser │  Monitor   │ │
│  └──────────┴──────────┴──────────┴────────────┘ │
│         │                      │                  │
│  ┌──────▼──────────────────────▼───────────────┐ │
│  │           Service Layer                      │ │
│  ├──────────┬──────────┬──────────┬────────────┤ │
│  │ WebSocket│  Push    │  Auth    │  Sync      │ │
│  │ Client   │ Notifs   │  (JWT)   │  Engine    │ │
│  └──────────┴──────────┴──────────┴────────────┘ │
└─────────────────────────────────────────────────┘
          │                    │
          ▼                    ▼
┌─────────────────┐  ┌──────────────────┐
│  Argentum GW     │  │  Push Services   │
│  (WebSocket +   │  │  (APNs / FCM)    │
│   REST API)     │  │                  │
└─────────────────┘  └──────────────────┘
```

## Feature Specifications

### 1. Push Notifications
- **APNs** (iOS) and **FCM** (Android) via Expo Notifications
- Notification types: chat messages, alerts, morning briefing, evening recap
- Rich notifications: images, action buttons, deep links
- Badge count sync with server
- Silent notifications for background data sync

### 2. Camera Access
- Take photos/videos and send to Argentum for multimodal analysis
- QR code scanning for device pairing
- Document scanning mode (auto-crop, enhance)
- Live preview with on-device ML for object detection
- Expo Camera API with custom overlay UI

### 3. GPS / Location
- Real-time location sharing with Argentum
- Geofencing triggers (arrive/leave locations)
- Background location tracking (opt-in, with clear privacy controls)
- Location history for context-aware recommendations
- Expo Location API with battery-optimized tracking

### 4. Screen Recording / Sharing
- Screen capture for support/debugging sessions
- Record screen as video and send to Argentum
- Frame extraction for real-time analysis
- Requires native modules (react-native-replay-kit on iOS, MediaProjection on Android)

### 5. Chat Interface
- Real-time WebSocket chat with Argentum
- Message types: text, images, voice, files, code blocks, cards
- Markdown rendering with syntax highlighting
- Typing indicators and read receipts
- Message search and history

### 6. Voice Interaction
- Push-to-talk and hands-free modes
- On-device STT for offline dictation
- TTS playback of Argentum responses
- Wake word detection ("Hey Claw")
- Audio streaming for real-time voice conversations

### 7. Offline Support
- Queue messages when offline, sync when connected
- Local SQLite cache for recent conversations
- Background sync via Expo BackgroundFetch
- Conflict resolution for concurrent edits

## Device Pairing

### Flow
1. User opens Argentum mobile app
2. App displays QR code or pairing code
3. User scans QR / enters code in Argentum webchat or CLI
4. Argentum generates JWT token with device ID
5. Mobile app stores token securely (Expo SecureStore)
6. WebSocket connection established with auth header

### Security
- Device-bound JWT tokens (1-year expiry, refreshable)
- Certificate pinning for API connections
- Biometric auth (Face ID / Touch ID / fingerprint) to unlock app
- Remote device wipe via Argentum admin command

## Directory Structure (Planned)

```
mobile/
├── app/                    # Expo Router screens
│   ├── (tabs)/
│   │   ├── chat.tsx        # Main chat screen
│   │   ├── camera.tsx      # Camera / QR scanner
│   │   ├── map.tsx         # Location / GPS
│   │   └── settings.tsx    # App settings
│   ├── pairing.tsx         # Device pairing flow
│   ├── _layout.tsx         # Root layout
│   └── +not-found.tsx
├── components/             # Reusable UI components
│   ├── ChatBubble.tsx
│   ├── MessageInput.tsx
│   ├── VoiceButton.tsx
│   ├── QRScanner.tsx
│   └── StatusIndicator.tsx
├── services/               # Business logic
│   ├── websocket.ts        # WS client with reconnection
│   ├── push-notifications.ts
│   ├── location.ts
│   ├── camera.ts
│   └── auth.ts
├── stores/                 # Zustand state stores
│   ├── chatStore.ts
│   ├── authStore.ts
│   └── settingsStore.ts
├── hooks/                  # Custom React hooks
│   ├── useWebSocket.ts
│   ├── useLocation.ts
│   └── useCamera.ts
├── utils/                  # Shared utilities
│   ├── api.ts
│   ├── storage.ts
│   └── types.ts            # Shared with Argentum backend
├── assets/                 # Images, fonts, sounds
├── app.json                # Expo config
├── package.json
├── tsconfig.json
└── README.md
```

## Development Setup

### Prerequisites
- Node.js >= 20
- Expo CLI: `npm install -g expo-cli`
- iOS Simulator (macOS) or Android Emulator
- Expo Go app on physical device for testing

### Commands
```bash
# Create project (from argentum root)
npx create-expo-app mobile --template tabs
cd mobile

# Install dependencies
npx expo install expo-camera expo-location expo-notifications \
  expo-secure-store expo-file-system expo-sharing \
  react-native-reanimated react-native-gesture-handler \
  zustand @tanstack/react-query

# Start development server
npx expo start

# Run on iOS simulator
npx expo run:ios

# Run on Android emulator
npx expo run:android

# Build for production (EAS)
npx eas build --platform all
```

## API Contract

The mobile app talks to Argentum via:

### WebSocket (Primary)
```
ws://<host>:18789/mobile/ws?token=<jwt>
```

Message format:
```json
{
  "type": "chat|command|status|media|location",
  "id": "msg_abc123",
  "payload": { ... },
  "timestamp": 1710691200000
}
```

### REST API (Secondary)
```
POST /api/mobile/pair        # Device pairing
POST /api/mobile/upload      # Media upload
GET  /api/mobile/sync        # Delta sync
POST /api/mobile/location    # Location update
GET  /api/mobile/config      # Get app config
```

## Privacy & Permissions

The app requests permissions progressively:
1. **On install:** None
2. **On pairing:** Network access
3. **On first camera use:** Camera permission (with explanation)
4. **On first location use:** Location permission (with explanation)
5. **On push notification setup:** Notification permission
6. **On screen recording:** Screen capture permission (iOS: ReplayKit)

All permissions include clear explanations and can be revoked in system settings. No permissions are required to use basic chat functionality.

## Timeline (Estimated)

| Phase | Features | Duration |
|-------|----------|----------|
| Phase 1 | Chat + Push + Pairing | 4 weeks |
| Phase 2 | Camera + QR Scanner | 3 weeks |
| Phase 3 | GPS + Geofencing | 2 weeks |
| Phase 4 | Voice (STT/TTS) | 3 weeks |
| Phase 5 | Screen Recording | 3 weeks |
| Phase 6 | Offline Support + Polish | 3 weeks |

**Total: ~18 weeks (4.5 months)** for full feature set.

Minimum viable app (Phase 1): 4 weeks.
