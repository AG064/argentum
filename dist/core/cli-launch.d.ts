export interface CliLaunchEnvironment {
    execPath: string;
    isPackaged?: boolean;
    platform: NodeJS.Platform | string;
}
export interface CliLaunch {
    command: string;
    pauseOnExit: boolean;
}
export interface GatewayChildProcessOptions {
    execPath: string;
    argv0?: string;
    entryPath?: string;
    isPackaged?: boolean;
    cliScriptPath: string;
    args: string[];
}
export interface GatewayChildProcess {
    command: string;
    args: string[];
}
export declare function resolveGatewayChildEnvironment(env: NodeJS.ProcessEnv, workDir: string): NodeJS.ProcessEnv;
export declare function resolveCliLaunch(args: string[], env: CliLaunchEnvironment): CliLaunch;
export declare function resolveGatewayChildProcess(options: GatewayChildProcessOptions): GatewayChildProcess;
//# sourceMappingURL=cli-launch.d.ts.map