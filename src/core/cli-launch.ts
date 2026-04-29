export interface CliLaunchEnvironment {
  execPath: string;
  isPackaged?: boolean;
  platform: NodeJS.Platform | string;
}

export interface CliLaunch {
  command: string;
  pauseOnExit: boolean;
}

function looksLikePackagedWindowsExe(execPath: string): boolean {
  return /(^|[\\/])argentum(?:-v\d+\.\d+\.\d+-win-x64)?\.exe$/i.test(execPath);
}

export function resolveCliLaunch(args: string[], env: CliLaunchEnvironment): CliLaunch {
  const explicitCommand = args[0];
  if (explicitCommand) {
    return { command: explicitCommand, pauseOnExit: false };
  }

  const packagedWindowsExe =
    env.platform === 'win32' && (env.isPackaged === true || looksLikePackagedWindowsExe(env.execPath));
  if (packagedWindowsExe) {
    return { command: 'launch', pauseOnExit: true };
  }

  return { command: 'help', pauseOnExit: false };
}
