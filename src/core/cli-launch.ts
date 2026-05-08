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

function firstNonEmpty(...values: Array<string | undefined>): string {
  return values.find((value) => value !== undefined && value.length > 0) ?? '';
}

export function resolveGatewayChildEnvironment(
  env: NodeJS.ProcessEnv,
  workDir: string,
): NodeJS.ProcessEnv {
  const childEnv = Object.fromEntries(
    Object.entries(env).filter(([key]) => !key.toUpperCase().startsWith('PKG_')),
  ) as NodeJS.ProcessEnv;
  childEnv.PKG_EXECPATH = '';
  childEnv.ARGENTUM_WORKDIR = workDir;
  childEnv.ARGENTUM_SKIP_EXIT_PAUSE = '1';
  childEnv.ARGENTUM_LOG_FORMAT = 'json';
  return childEnv;
}

export function resolveCliLaunch(args: string[], env: CliLaunchEnvironment): CliLaunch {
  const explicitCommand = args[0];
  if (explicitCommand) {
    return { command: explicitCommand, pauseOnExit: false };
  }

  return { command: 'help', pauseOnExit: false };
}

export function resolveGatewayChildProcess(
  options: GatewayChildProcessOptions,
): GatewayChildProcess {
  if (options.isPackaged) {
    const entryPath = options.entryPath ?? '';
    const packagedEntrypoint =
      entryPath && !entryPath.includes('\\snapshot\\') && !entryPath.includes('/snapshot/')
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
