export interface CliLaunchEnvironment {
    execPath: string;
    isPackaged?: boolean;
    platform: NodeJS.Platform | string;
}
export interface CliLaunch {
    command: string;
    pauseOnExit: boolean;
}
export declare function resolveCliLaunch(args: string[], env: CliLaunchEnvironment): CliLaunch;
//# sourceMappingURL=cli-launch.d.ts.map