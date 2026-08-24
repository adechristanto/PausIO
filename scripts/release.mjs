#!/usr/bin/env node
// Interactive semantic-release script. Prompts patch / minor / major, bumps
// src-tauri/tauri.conf.json, rewrites CHANGELOG.md (Keep a Changelog format),
// then creates an annotated git tag.
//
// Node, not POSIX shell (see scripts/tauri.mjs for the same rationale):
// `pnpm run` invokes package scripts from the repository root on Windows,
// where there is no `sh` to execute a `.sh` file directly.
import { execFileSync } from 'node:child_process'
import { readFileSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { createInterface } from 'node:readline'

const repoDir = dirname(dirname(fileURLToPath(import.meta.url)))
const tauriConfPath = join(repoDir, 'src-tauri', 'tauri.conf.json')
const changelogPath = join(repoDir, 'CHANGELOG.md')

// ── helpers ──────────────────────────────────────────────────────────
function die(message) {
  process.stderr.write(`error: ${message}\n\n`)
  process.exit(1)
}
function info(message) {
  console.log(`\x1b[32m✓\x1b[0m ${message}`)
}
function warn(message) {
  console.log(`\x1b[33m⚠\x1b[0m ${message}`)
}
function ask(message) {
  console.log(`\x1b[1m❯\x1b[0m ${message}`)
}

function git(args) {
  return execFileSync('git', args, { cwd: repoDir, encoding: 'utf8' })
}

function readVersion(path) {
  const raw = readFileSync(path, 'utf8')
  const match = raw.match(/"version"\s*:\s*"(\d+\.\d+\.\d+)"/)
  if (!match) die(`Could not find a "version": "x.y.z" field in ${path}`)
  return match[1]
}

// The original shell implementation of this (`tr '.' ' '` then
// `IFS='.' read`) never actually worked: converting the dots to spaces
// first leaves nothing for the following dot-delimited `read` to split on,
// so it collapses the whole string into one field. Parse the three
// numeric segments directly instead.
function bumpVersion(current, bumpType) {
  const match = current.match(/^(\d+)\.(\d+)\.(\d+)$/)
  if (!match) die(`Current version "${current}" is not a valid x.y.z semantic version.`)
  let [major, minor, patch] = match.slice(1).map(Number)
  if (bumpType === 'major') {
    major += 1
    minor = 0
    patch = 0
  } else if (bumpType === 'minor') {
    minor += 1
    patch = 0
  } else {
    patch += 1
  }
  return `${major}.${minor}.${patch}`
}

function localIsoDate() {
  const now = new Date()
  const pad = (n) => String(n).padStart(2, '0')
  return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`
}

// node:readline's `rl.question()` registers its callback only at the moment
// it's called. When answers are piped in as a single chunk (a scripted or
// dry-run invocation) rather than typed live, `line` events for input
// beyond the first question can already be emitted before the second
// `rl.question()` call runs and starts listening — silently dropping that
// answer, after which the process hangs on a promise that never resolves
// once stdin closes (Node then exits early with "unsettled top-level
// await"). Queuing every `line` event as soon as the interface is created,
// before any prompt is shown, avoids that race regardless of timing.
function createPrompter(rl) {
  const queue = []
  const waiters = []
  rl.on('line', (line) => {
    const waiter = waiters.shift()
    if (waiter) waiter(line)
    else queue.push(line)
  })
  return function question(prompt) {
    if (prompt) process.stdout.write(prompt)
    if (queue.length > 0) return Promise.resolve(queue.shift())
    return new Promise((resolve) => waiters.push(resolve))
  }
}

async function main() {
  // ── pre-flight ───────────────────────────────────────────────────────
  if (git(['status', '--porcelain']).trim() !== '') {
    die('Working tree is dirty. Commit or stash your changes before releasing.')
  }

  const currentBranch = git(['rev-parse', '--abbrev-ref', 'HEAD']).trim()
  const rl = createInterface({ input: process.stdin, output: process.stdout })
  const question = createPrompter(rl)
  try {
    if (currentBranch !== 'main') {
      warn(`You are on branch "${currentBranch}", not "main". Do you want to continue? [y/N]`)
      const ans = (await question('')).trim()
      if (!/^y(es)?$/i.test(ans)) die('Aborted.')
    }

    const currentVersion = readVersion(tauriConfPath)
    ask(`Current version: ${currentVersion}`)

    // ── choose bump ─────────────────────────────────────────────────────
    console.log(`\n  [1] Patch  (${currentVersion} → )`)
    console.log('  [2] Minor')
    console.log('  [3] Major')
    const choice = (await question('Choice [1-3]: ')).trim()
    const bumpType = { 1: 'patch', 2: 'minor', 3: 'major' }[choice]
    if (!bumpType) die('Invalid choice.')

    const newVersion = bumpVersion(currentVersion, bumpType)
    const tagName = `v${newVersion}`

    ask(`Bumping ${bumpType}: ${currentVersion} → ${newVersion}`)

    // ── update tauri.conf.json ───────────────────────────────────────────
    // A targeted regex replace, not a full JSON.parse/stringify round trip,
    // to preserve the file's existing formatting exactly (matches the
    // original script's approach).
    const confRaw = readFileSync(tauriConfPath, 'utf8')
    const confUpdated = confRaw.replace(/("version"\s*:\s*")\d+\.\d+\.\d+(")/, `$1${newVersion}$2`)
    writeFileSync(tauriConfPath, confUpdated)
    info(`Updated src-tauri/tauri.conf.json → ${newVersion}`)

    // ── update CHANGELOG.md ─────────────────────────────────────────────
    const today = localIsoDate()
    const lines = readFileSync(changelogPath, 'utf8').split('\n')

    const unreleasedIndex = lines.findIndex((line) => /^## \[Unreleased\]/.test(line))
    if (unreleasedIndex === -1) {
      die(`Could not find a "## [Unreleased]" section in ${changelogPath}`)
    }
    const header = lines.slice(0, unreleasedIndex + 1).join('\n')

    // Next "## " line after Unreleased ends that section. Unlike the
    // original shell loop, that line is kept (as the start of `remaining`)
    // rather than silently dropped — the shell version's state machine
    // consumed it only to flip flags, deleting the previous release's own
    // heading from the changelog on every run.
    let sectionEnd = lines.length
    for (let i = unreleasedIndex + 1; i < lines.length; i += 1) {
      if (lines[i].startsWith('## ')) {
        sectionEnd = i
        break
      }
    }
    const unreleasedBody = lines.slice(unreleasedIndex + 1, sectionEnd).join('\n')
    const remaining = lines.slice(sectionEnd).join('\n')

    // Collect untagged commits for the new release entry. If no previous
    // tag exists, collect all commits up to HEAD.
    const prevTag = git(['tag', '--sort=-v:refname'])
      .split('\n')
      .find((line) => line.trim() !== '')
    const commitRange = prevTag ? [`${prevTag}..HEAD`] : []
    let commits = git(['log', ...commitRange, '--oneline', '--no-merges']).trim()
    if (commits === '') commits = '(no commits since project start)'

    const sections = [
      header,
      `## [${tagName}] - ${today}`,
      '',
      '### Added',
      '',
      '### Fixed',
      '',
      '### Changed',
      '',
      '- Auto-generated release notes from git log:',
      ...commits.split('\n').map((line) => `  - ${line}`),
    ]
    let newContent = `${sections.join('\n')}\n`
    if (unreleasedBody !== '') newContent += `\n${unreleasedBody}\n`
    if (remaining !== '') newContent += remaining

    writeFileSync(changelogPath, newContent)
    info('Updated CHANGELOG.md')

    // ── create tag ───────────────────────────────────────────────────────
    git(['add', 'src-tauri/tauri.conf.json', 'CHANGELOG.md'])
    try {
      execFileSync('git', ['commit', '-m', `bump version ${newVersion}`, '--no-verify'], {
        cwd: repoDir,
        stdio: 'ignore',
      })
    } catch {
      // Matches the original script: a failed/empty commit is not fatal.
    }
    git(['tag', '-a', tagName, '-m', `Release ${newVersion}`])
    info(`Created tag ${tagName}`)

    info('Release complete. To share:')
    console.log(`  \x1b[1mgit push origin ${currentBranch}\x1b[0m`)
    console.log(`  \x1b[1mgit push origin ${tagName}\x1b[0m`)
  } finally {
    rl.close()
  }
}

await main()
