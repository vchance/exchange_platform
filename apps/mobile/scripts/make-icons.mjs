// Draws the Yuppers mark, a speech bubble with a "Y" in it, and writes every
// image made from it: the web app's favicon and the mobile app's icon, the
// layers of its Android adaptive icon, the splash image, and the icon and
// logo of an Apple Wallet pass (backend/assets/wallet). The mark is defined
// once, below, so the images cannot drift apart.
//
//   node apps/mobile/scripts/make-icons.mjs
//
// Needs `rsvg-convert` on the PATH (librsvg; `brew install librsvg` on macOS,
// `apt-get install librsvg2-bin` on Debian). No fonts are involved: the "Y" is
// two strokes, so the output is the same on every machine.

import { spawnSync } from 'node:child_process'
import { mkdirSync, writeFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const mobile = join(dirname(fileURLToPath(import.meta.url)), '..')
const assets = join(mobile, 'assets')
const favicon = join(mobile, '..', 'web', 'public', 'favicon.svg')
const wallet = join(mobile, '..', '..', 'backend', 'assets', 'wallet')

/** The app's accent colour (apps/web/src/index.css, apps/mobile/src/lib/theme.ts). */
const BLUE = '#0b57b0'
const WHITE = '#ffffff'

// The mark on a 64 by 64 grid: a rounded bubble with its tail at the lower
// left, and a "Y" centred in the body of the bubble.
const BUBBLE =
  'M18 4H46A14 14 0 0 1 60 18V36A14 14 0 0 1 46 50H27L14 60L18 50A14 14 0 0 1 4 36V18A14 14 0 0 1 18 4Z'
const Y = 'M22 15L32 27L42 15M32 27V39'

/** The mark, scaled by `scale` about the centre of a `size` square. */
function mark({ size, scale, bubble, letter, knockout = false }) {
  const offset = (size - 64 * scale) / 2
  const transform = `translate(${offset} ${offset}) scale(${scale})`
  const stroke = `stroke-width="7" stroke-linecap="round" stroke-linejoin="round" fill="none"`
  if (knockout) {
    // One colour, the "Y" cut out of the bubble: for the monochrome layer,
    // which Android tints and reads only for its shape.
    return `<mask id="y"><rect width="64" height="64" fill="#fff"/><path d="${Y}" stroke="#000" ${stroke}/></mask>
  <g transform="${transform}"><path d="${BUBBLE}" fill="${bubble}" mask="url(#y)"/></g>`
  }
  return `<g transform="${transform}"><path d="${BUBBLE}" fill="${bubble}"/><path d="${Y}" stroke="${letter}" ${stroke}/></g>`
}

function svg(size, body, background) {
  const fill = background ? `<rect width="${size}" height="${size}" fill="${background}"/>` : ''
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${size}" height="${size}" viewBox="0 0 ${size} ${size}">
  ${fill}${body}
</svg>
`
}

function png(name, size, source, directory = assets) {
  const result = spawnSync('rsvg-convert', ['--width', String(size), '--height', String(size), '--format', 'png'], {
    input: source,
    maxBuffer: 64 * 1024 * 1024,
  })
  if (result.error || result.status !== 0) {
    throw new Error(`rsvg-convert failed for ${name}: ${result.error ?? result.stderr}`)
  }
  writeFileSync(join(directory, name), result.stdout)
  console.log(`wrote ${directory === assets ? 'assets/' : 'backend/assets/wallet/'}${name} (${size}x${size})`)
}

// The favicon: the mark alone, blue with a white "Y", which reads on light
// and dark browser tabs alike.
writeFileSync(favicon, svg(64, mark({ size: 64, scale: 1, bubble: BLUE, letter: WHITE })))
console.log('wrote apps/web/public/favicon.svg')

// The app icon, opaque and full-bleed as the stores require: a white bubble
// on the accent colour.
png('icon.png', 1024, svg(1024, mark({ size: 1024, scale: 10, bubble: WHITE, letter: BLUE }), BLUE))

// Android's adaptive icon. The launcher masks it to its own shape and shows
// only the middle two thirds for sure, so the mark stays inside that.
png('android-icon-background.png', 512, svg(512, '', BLUE))
png('android-icon-foreground.png', 512, svg(512, mark({ size: 512, scale: 4.25, bubble: WHITE, letter: BLUE })))
png('android-icon-monochrome.png', 432, svg(432, mark({ size: 432, scale: 3.6, bubble: WHITE, knockout: true })))

// The splash image: the mark as in the favicon, on a transparent ground.
png('splash-icon.png', 1024, svg(1024, mark({ size: 1024, scale: 9, bubble: BLUE, letter: WHITE })))

// An Apple Wallet pass, at the three scales Wallet asks for. The icon stands
// for the pass in notifications and on the lock screen, so it is the app
// icon again, 38 points square. The logo sits at the top of the pass, which
// has the accent colour behind it (backend/src/wallet/apple.rs): the white
// bubble alone on a transparent ground, 50 points square, within the 160 by
// 50 Wallet allows.
mkdirSync(wallet, { recursive: true })
for (const [suffix, factor] of [['', 1], ['@2x', 2], ['@3x', 3]]) {
  const icon = 38 * factor
  png(`icon${suffix}.png`, icon, svg(icon, mark({ size: icon, scale: (icon / 64) * 0.8, bubble: WHITE, letter: BLUE }), BLUE), wallet)
  const logo = 50 * factor
  png(`logo${suffix}.png`, logo, svg(logo, mark({ size: logo, scale: logo / 64, bubble: WHITE, letter: BLUE })), wallet)
}
