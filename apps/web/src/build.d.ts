/** This build's version, put in by `vite.config.ts` from `package.json`. */
declare const __WEB_VERSION__: string
/**
 * The git commit this build was made from, put in by `vite.config.ts` from
 * `GIT_SHA` (or Render's `RENDER_GIT_COMMIT`), or `null` when the build's
 * environment did not say.
 */
declare const __WEB_COMMIT__: string | null
