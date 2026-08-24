import { resolve } from 'node:path'

const executable = process.platform === 'win32' ? 'pausio.exe' : 'pausio'
const appBinaryPath = resolve(process.cwd(), 'target/debug', executable)

export const config = {
  runner: 'local',
  specs: ['./tests/e2e/**/*.e2e.ts'],
  maxInstances: 1,
  logLevel: 'warn',
  framework: 'mocha',
  waitforTimeout: 10_000,
  connectionRetryTimeout: 60_000,
  connectionRetryCount: 1,
  services: [
    [
      'tauri',
      {
        appBinaryPath,
        appArgs: ['--e2e'],
        driverProvider: 'embedded',
        autoInstallTauriDriver: true,
      },
    ],
  ],
  capabilities: [
    {
      browserName: 'tauri',
      'tauri:options': { application: appBinaryPath, arguments: ['--e2e'] },
    },
  ],
  mochaOpts: { ui: 'bdd', timeout: 60_000 },
}
