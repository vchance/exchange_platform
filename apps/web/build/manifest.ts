import { mkdirSync, renameSync, rmSync } from 'node:fs'
import { dirname, isAbsolute, join } from 'node:path'

import type { Plugin } from 'vite'

/*
 * Vite's build manifest says which files each entry and each lazy chunk
 * loads, which is what `scripts/check-budget.mjs` adds up. It is wanted next
 * to the build, not in it: whatever is in `dist` is served to everyone, and
 * the manifest is of use to nobody there. So the build writes it (with
 * `build.manifest`) and this moves it out once it is on disk.
 */
export const MANIFEST_IN_BUILD = '.vite/manifest.json'

export function manifestOutPlugin(destination: string): Plugin {
  let outDir = ''
  return {
    name: 'exchange:manifest-out',
    apply: 'build',
    enforce: 'post',
    configResolved(config) {
      const { outDir: configured } = config.build
      outDir = isAbsolute(configured) ? configured : join(config.root, configured)
    },
    writeBundle() {
      const written = join(outDir, MANIFEST_IN_BUILD)
      mkdirSync(dirname(destination), { recursive: true })
      renameSync(written, destination)
      rmSync(dirname(written), { recursive: true, force: true })
    },
  }
}
