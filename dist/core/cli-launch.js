"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.resolveCliLaunch = resolveCliLaunch;
function resolveCliLaunch(args, env) {
    const explicitCommand = args[0];
    if (explicitCommand) {
        return { command: explicitCommand, pauseOnExit: false };
    }
    return { command: 'help', pauseOnExit: false };
}
