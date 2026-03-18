"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const container_sandbox_1 = __importDefault(require("../src/features/container-sandbox"));
describe('container-sandbox validation (exec-level)', () => {
    test('forbidden command blocked (rm -rf /)', async () => {
        await expect(container_sandbox_1.default.execute('rm -rf /')).rejects.toThrow();
    });
    test('dangerous chars blocked', async () => {
        await expect(container_sandbox_1.default.execute('ls; curl evil.com')).rejects.toThrow();
    });
    test('ls is accepted by validator (may fail runtime without docker)', async () => {
        const res = await container_sandbox_1.default.execute('ls');
        // When docker isn't available the feature returns success:false but does not throw validation error
        expect(res).toHaveProperty('success');
        expect(typeof res.success).toBe('boolean');
    });
    test('parsing with quotes handled (no validation error)', async () => {
        await expect(container_sandbox_1.default.execute("echo 'hello world'")).resolves.toHaveProperty('success');
    });
});
