import {
  resolveCliLaunch,
  resolveGatewayChildEnvironment,
  resolveGatewayChildProcess,
} from '../src/core/cli-launch';

describe('CLI launch resolution', () => {
  test('keeps no-argument packaged Windows CLI executables in terminal help mode', () => {
    const launch = resolveCliLaunch([], {
      execPath: 'C:\\Program Files\\Argentum\\argentum-cli-v0.0.4-win-x64.exe',
      isPackaged: true,
      platform: 'win32',
    });

    expect(launch.command).toBe('help');
    expect(launch.pauseOnExit).toBe(false);
  });

  test('keeps normal developer CLI no-argument behavior as help', () => {
    const launch = resolveCliLaunch([], {
      execPath: process.execPath,
      isPackaged: false,
      platform: process.platform,
    });

    expect(launch.command).toBe('help');
    expect(launch.pauseOnExit).toBe(false);
  });

  test('does not hijack explicit commands for packaged Windows executables', () => {
    const launch = resolveCliLaunch(['--version'], {
      execPath: 'C:\\Program Files\\Argentum\\argentum-cli-v0.0.4-win-x64.exe',
      isPackaged: true,
      platform: 'win32',
    });

    expect(launch.command).toBe('--version');
    expect(launch.pauseOnExit).toBe(false);
  });

  test('does not treat legacy agclaw executable names as Argentum launchers', () => {
    const launch = resolveCliLaunch([], {
      execPath: 'C:\\Program Files\\Argentum\\agclaw.exe',
      isPackaged: false,
      platform: 'win32',
    });

    expect(launch.command).toBe('help');
    expect(launch.pauseOnExit).toBe(false);
  });

  test('packaged gateway start uses packaged entry executable instead of node cli.js', () => {
    const child = resolveGatewayChildProcess({
      execPath: 'C:\\Program Files\\nodejs\\node.exe',
      argv0: 'C:\\Program Files\\nodejs\\node.exe',
      entryPath: 'C:\\Program Files\\Argentum\\argentum-cli.exe',
      isPackaged: true,
      cliScriptPath: 'C:\\snapshot\\argentum\\dist\\cli.js',
      args: ['start', '--port', '3000'],
    });

    expect(child.command).toBe('C:\\Program Files\\Argentum\\argentum-cli.exe');
    expect(child.args).toEqual(['start', '--port', '3000']);
    expect(child.args).not.toContain('C:\\snapshot\\argentum\\dist\\cli.js');
  });

  test('developer gateway start keeps using node with the compiled CLI script', () => {
    const child = resolveGatewayChildProcess({
      execPath: 'C:\\Program Files\\nodejs\\node.exe',
      isPackaged: false,
      cliScriptPath: 'A:\\ag064\\ag-claw\\ag-claw\\dist\\cli.js',
      args: ['start', '--port', '3000'],
    });

    expect(child.command).toBe('node');
    expect(child.args).toEqual([
      'A:\\ag064\\ag-claw\\ag-claw\\dist\\cli.js',
      'start',
      '--port',
      '3000',
    ]);
  });

  test('gateway child environment strips pkg executable bootstrap variables', () => {
    const env = resolveGatewayChildEnvironment(
      {
        PKG_EXECPATH: 'C:\\Program Files\\Argentum\\argentum-cli.exe',
        pkg_cache_path: 'C:\\pkg-cache',
        ARGENTUM_WORKDIR: 'C:\\old',
      },
      'C:\\workspace',
    );

    expect(env['PKG_EXECPATH']).toBe('');
    expect(env['pkg_cache_path']).toBeUndefined();
    expect(env['ARGENTUM_WORKDIR']).toBe('C:\\workspace');
    expect(env['ARGENTUM_SKIP_EXIT_PAUSE']).toBe('1');
    expect(env['ARGENTUM_LOG_FORMAT']).toBe('json');
  });
});
