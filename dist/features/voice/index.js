"use strict";
/**
 * Voice Feature for Argentum
 *
 * Text-to-Speech (TTS), Speech-to-Text (STT) via ElevenLabs and OpenAI.
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.generateSpeech = generateSpeech;
exports.listVoices = listVoices;
exports.getVoice = getVoice;
// ─── ElevenLabs API helpers ────────────────────────────────────────────────────
const ELEVENLABS_API = 'https://api.elevenlabs.io/v1';
/** Generate speech using ElevenLabs API */
async function generateSpeech(text, voiceId, options) {
    const apiKey = process.env.ELEVENLABS_API_KEY;
    if (!apiKey) {
        throw new Error('ELEVENLABS_API_KEY environment variable is required');
    }
    const voice = voiceId ?? process.env.ELEVENLABS_DEFAULT_VOICE ?? 'JBFqnCBsd6RMkjVDRZzb'; // George
    const model = options?.model ?? 'eleven_multilingual_v2';
    const response = await fetch(`${ELEVENLABS_API}/text-to-speech/${voice}`, {
        method: 'POST',
        headers: {
            'xi-api-key': apiKey,
            'Content-Type': 'application/json',
            'Accept': 'audio/mpeg',
        },
        body: JSON.stringify({
            text,
            model_id: model,
            voice_settings: {
                stability: options?.stability ?? 0.5,
                similarity_boost: options?.similarityBoost ?? 0.75,
            },
        }),
    });
    if (!response.ok) {
        const errorText = await response.text();
        throw new Error(`ElevenLabs TTS error (${response.status}): ${errorText}`);
    }
    return Buffer.from(await response.arrayBuffer());
}
/** List available voices from ElevenLabs */
async function listVoices() {
    const apiKey = process.env.ELEVENLABS_API_KEY;
    if (!apiKey) {
        throw new Error('ELEVENLABS_API_KEY environment variable is required');
    }
    const response = await fetch(`${ELEVENLABS_API}/voices`, {
        method: 'GET',
        headers: {
            'xi-api-key': apiKey,
        },
    });
    if (!response.ok) {
        const errorText = await response.text();
        throw new Error(`ElevenLabs voices API error (${response.status}): ${errorText}`);
    }
    const data = (await response.json());
    return data.voices.map((v) => ({
        id: v.voice_id,
        name: v.name,
        category: v.category,
        description: v.description,
        labels: v.labels,
    }));
}
/** Get voice info by ID or name */
async function getVoice(voiceId) {
    const voices = await listVoices();
    return (voices.find((v) => v.id === voiceId || v.name.toLowerCase() === voiceId.toLowerCase()) ?? null);
}
// ─── Feature Module ───────────────────────────────────────────────────────────
class VoiceFeature {
    meta = {
        name: 'voice',
        version: '0.0.4',
        description: 'Text-to-Speech and Speech-to-Text via ElevenLabs and OpenAI',
        dependencies: [],
    };
    config = {
        enabled: false,
        provider: 'elevenlabs',
        voice: 'default',
        model: 'eleven_multilingual_v2',
        sttProvider: 'whisper',
    };
    ctx;
    async init(config, context) {
        this.ctx = context;
        this.config = {
            ...this.config,
            ...config,
        };
        this.ctx.logger.info('Voice feature initialized', {
            provider: this.config.provider,
            voice: this.config.voice,
        });
    }
    async start() {
        // Verify API key is available
        if (this.config.provider === 'elevenlabs' && !process.env.ELEVENLABS_API_KEY) {
            this.ctx.logger.warn('ELEVENLABS_API_KEY not set — TTS will not work');
        }
        if (this.config.sttProvider === 'whisper' && !process.env.OPENAI_API_KEY) {
            this.ctx.logger.warn('OPENAI_API_KEY not set — STT will not work');
        }
        this.ctx.logger.info('Voice feature active');
    }
    async stop() {
        // Nothing to clean up
    }
    async healthCheck() {
        const hasTTS = this.config.provider === 'elevenlabs'
            ? !!process.env.ELEVENLABS_API_KEY
            : !!process.env.OPENAI_API_KEY;
        const hasSTT = this.config.sttProvider === 'whisper' ? !!process.env.OPENAI_API_KEY : true;
        return {
            healthy: hasTTS && hasSTT,
            details: {
                provider: this.config.provider,
                sttProvider: this.config.sttProvider,
                ttsConfigured: hasTTS,
                sttConfigured: hasSTT,
            },
        };
    }
    /** Generate speech audio */
    async textToSpeech(text, voiceId) {
        return generateSpeech(text, voiceId ?? this.config.voice, {
            model: this.config.model,
        });
    }
    /** List available voices */
    async getVoices() {
        if (this.config.provider === 'elevenlabs') {
            return listVoices();
        }
        return [];
    }
    /** Transcribe audio */
    async speechToText(audio, format = 'ogg') {
        const openaiKey = process.env.OPENAI_API_KEY;
        if (!openaiKey) {
            throw new Error('OPENAI_API_KEY not configured');
        }
        const formData = new FormData();
        const blob = new Blob([new Uint8Array(audio)], { type: `audio/${format}` });
        formData.append('file', blob, `audio.${format}`);
        formData.append('model', 'whisper-1');
        const response = await fetch('https://api.openai.com/v1/audio/transcriptions', {
            method: 'POST',
            headers: { Authorization: `Bearer ${openaiKey}` },
            body: formData,
        });
        if (!response.ok) {
            throw new Error(`Whisper API error: ${response.status}`);
        }
        const data = (await response.json());
        return data.text;
    }
}
exports.default = new VoiceFeature();
//# sourceMappingURL=index.js.map