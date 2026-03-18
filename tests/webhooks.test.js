"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const webhooks_1 = __importDefault(require("../src/features/webhooks"));
describe('webhooks URL validation and blocking', () => {
    const instance = webhooks_1.default;
    test('https://example.com passes', () => {
        const ok = instance.validateUrl('https://example.com');
        expect(ok).toBe(true);
    });
    test('localhost blocked', () => {
        const ok = instance.validateUrl('http://localhost:8080');
        expect(ok).toBe(false);
    });
    test('private IP blocked', () => {
        expect(instance.validateUrl('http://10.0.0.1')).toBe(false);
        expect(instance.validateUrl('http://169.254.169.254')).toBe(false);
    });
    test('signature verification', () => {
        const payload = JSON.stringify({ foo: 'bar' });
        const secret = 's3cr3t';
        const sig = require('crypto').createHmac('sha256', secret).update(payload).digest('hex');
        expect(instance.verifySignature(payload, `sha256=${sig}`, secret)).toBe(true);
        expect(instance.verifySignature(payload, `sha256=bad`, secret)).toBe(false);
    });
});
