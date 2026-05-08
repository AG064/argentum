"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.resolveGatewayChildEnvironment = resolveGatewayChildEnvironment;
exports.resolveCliLaunch = resolveCliLaunch;
exports.resolveGatewayChildProcess = resolveGatewayChildProcess;
function firstNonEmpty(...values) {
    return values.find((value) => value !== undefined && value.length > 0) ?? '';
}
function resolveGatewayChildEnvironment(env, workDir, extraEnv) {
    const childEnv = Object.fromEntries(Object.entries(env).filter(([key]) => !key.toUpperCase().startsWith('PKG_')));
    if (extraEnv)
        Object.assign(childEnv, extraEnv);
    childEnv.PKG_EXECPATH = '';
    childEnv.ARGENTUM_WORKDIR = workDir;
    childEnv.ARGENTUM_SKIP_EXIT_PAUSE = '1';
    childEnv.ARGENTUM_LOG_FORMAT = 'json';
    childEnv.ARGENTUM_NO_BANNER = '1';
    childEnv.ARGENTUM_PLAIN_OUTPUT = '1';
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
        const entryPath = options.entryPath ?? '';
        const packagedEntrypoint = entryPath && !entryPath.includes('\\snapshot\\') && !entryPath.includes('/snapshot/')
            ? entryPath
            : '';
        return {
            command: firstNonEmpty(packagedEntrypoint, options.argv0, options.execPath),
            args: [...options.args],
        };
    }
    return {
        command: 'node',
        args: [options.cliScriptPath, ...options.args],
    };
}
