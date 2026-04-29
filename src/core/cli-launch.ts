export interface CliLaunchEnvironment {
  execPath: string;
  isPackaged?: boolean;
  platform: NodeJS.Platform | string;
}

export interface CliLaunch {
  command: string;
  pauseOnExit: boolean;
}

export function resolveCliLaunch(args: string[], env: CliLaunchEnvironment): CliLaunch {
  const explicitCommand = args[0];
  if (explicitCommand) {
    return { command: explicitCommand, pauseOnExit: false };
  }

  return { command: 'help', pauseOnExit: false };
}
