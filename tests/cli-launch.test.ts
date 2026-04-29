import { resolveCliLaunch } from '../src/core/cli-launch';

describe('CLI launch resolution', () => {
  test('routes a no-argument packaged Windows executable to the launch flow', () => {
    const launch = resolveCliLaunch([], {
      execPath: 'C:\\Program Files\\Argentum\\argentum.exe',
      isPackaged: true,
      platform: 'win32',
    });

    expect(launch.command).toBe('launch');
    expect(launch.pauseOnExit).toBe(true);
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
      execPath: 'C:\\Program Files\\Argentum\\argentum.exe',
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
});
