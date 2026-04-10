"use strict";
/**
 * Budget Feature Types
 */
Object.defineProperty(exports, "__esModule", { value: true });
exports.MODEL_PRICING = void 0;
exports.MODEL_PRICING = {
    'minimax/m2': { input: 0.1, output: 0.5 },
    'openai/gpt-4o': { input: 2.5, output: 10 },
    'openai/gpt-4o-mini': { input: 0.15, output: 0.6 },
    'openai/gpt-4-turbo': { input: 10, output: 30 },
    'anthropic/claude-sonnet-4-20250514': { input: 3, output: 15 },
    'anthropic/claude-opus-4-20250514': { input: 15, output: 75 },
    'anthropic/claude-3-5-sonnet-latest': { input: 3, output: 15 },
    'anthropic/claude-3-5-haiku-latest': { input: 0.8, output: 4 },
    'google/gemini-2.5-flash': { input: 0.1, output: 0.4 },
    'google/gemini-2.5-pro': { input: 1.25, output: 5 },
    'nvidia/*': { input: 0, output: 0 },
    'openrouter/free': { input: 0, output: 0 },
    'deepseek-ai/deepseek-v3.2': { input: 0.1, output: 0.5 },
};
//# sourceMappingURL=types.js.map