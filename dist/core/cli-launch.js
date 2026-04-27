"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.resolveCliLaunch = resolveCliLaunch;
function looksLikePackagedWindowsExe(execPath) {
    return /(^|[\\/])(?:argentum|agclaw)(?:-v\d+\.\d+\.\d+-win-x64)?\.exe$/i.test(execPath);
}
function resolveCliLaunch(args, env) {
    const explicitCommand = args[0];
    if (explicitCommand) {
        return { command: explicitCommand, pauseOnExit: false };
    }
    const packagedWindowsExe = env.platform === 'win32' && (env.isPackaged === true || looksLikePackagedWindowsExe(env.execPath));
    if (packagedWindowsExe) {
        return { command: 'launch', pauseOnExit: true };
    }
    return { command: 'help', pauseOnExit: false };
}
