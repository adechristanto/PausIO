#!/usr/bin/env node
// Cross-platform root launcher for the Tauri CLI. `pnpm` package scripts run
// from the repository root on Windows, so a shell wrapper that `cd`s into
// src-tauri cannot be the public command contract.
import { spawnSync } from 'node:child_process'
import { existsSync, mkdtempSync, renameSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const repository = dirname(dirname(fileURLToPath(import.meta.url)))
const nativeDirectory = join(repository, 'src-tauri')
const cli = join(repository, 'frontend', 'node_modules', '@tauri-apps', 'cli', 'tauri.js')
const args = process.argv.slice(2)

if (!existsSync(cli)) {
  throw new Error('PausIO dependencies are missing. Run pnpm install first.')
}

// Tauri's iOS generator can leave a stale host build directory that it then
// tries to reuse. Keep the existing recovery behavior, but use Node APIs so
// it is equally safe when invoked from Windows CI.
if (args[0] === 'ios' && args[1] === 'build') {
  const generatedBuild = join(nativeDirectory, 'gen', 'apple', 'build')
  if (existsSync(generatedBuild)) {
    const recovery = mkdtempSync(join(tmpdir(), 'pausio-ios-build-'))
    renameSync(generatedBuild, join(recovery, 'build'))
  }
}

const result = spawnSync(process.execPath, [cli, ...args], {
  cwd: nativeDirectory,
  stdio: 'inherit',
  env: process.env,
})

if (result.error) throw result.error
process.exit(result.status ?? 1)
