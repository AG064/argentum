"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.resolveGatewayChildEnvironment = resolveGatewayChildEnvironment;
exports.resolveCliLaunch = resolveCliLaunch;
exports.resolveGatewayChildProcess = resolveGatewayChildProcess;
function resolveGatewayChildEnvironment(env, workDir) {
    const childEnv = { ...env };
    for (const key of Object.keys(childEnv)) {
        if (key.toUpperCase().startsWith('PKG_')) {
            delete childEnv[key];
        }
    }
    childEnv.PKG_EXECPATH = '';
    childEnv.ARGENTUM_WORKDIR = workDir;
    childEnv.ARGENTUM_SKIP_EXIT_PAUSE = '1';
    childEnv.ARGENTUM_LOG_FORMAT = 'json';
    return childEnv;
}
function resolveCliLaunch(args, env) {
    const explicitCommand = args[0];
    if (explicitCommand) {
        return { command: explicitCommand, pauseOnExit: false };
    }
    return { command: 'help', pauseOnExit: false };
}
function resolveGatewayChildProcess(options) {
    if (options.isPackaged) {
        const entryPath = options.entryPath || '';
        const packagedEntrypoint = entryPath && !entryPath.includes('\\snapshot\\') && !entryPath.includes('/snapshot/')
            ? entryPath
            : '';
        return {
            command: packagedEntrypoint || options.argv0 || options.execPath,
            args: [...options.args],
        };
    }
    return {
        command: 'node',
        args: [options.cliScriptPath, ...options.args],
    };
}
