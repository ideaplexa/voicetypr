// Actual CSS/layout coverage, unlike jsdom's zero-sized client rects.
// Run: node scripts/test-pill-card-actions.mjs (Chrome, or CHROME_PATH).
// This launches only an isolated headless browser, never the Tauri app.
import assert from 'node:assert/strict';
import { chromium } from 'playwright-core';
import { createServer } from 'vite';

const server = await createServer({ server: { host: '127.0.0.1', port: 0 } });
server.middlewares.use((req, res, next) => {
  if (req.url !== '/__pill_card_test') return next();
  res.setHeader('Content-Type', 'text/html');
  res.end('<!doctype html><html><body></body></html>');
});
let browser;
try {
  await server.listen();
  browser = await chromium.launch({
    executablePath: process.env.CHROME_PATH ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    headless: true,
  });
  let checked = 0;
  for (const scale of [1, 2]) {
    const page = await browser.newPage({ viewport: { width: 462, height: 442 }, deviceScaleFactor: scale });
    await page.goto(`${server.resolvedUrls.local[0]}__pill_card_test`);
    const result = await page.evaluate(async () => {
      await import('/src/pill.css');
      await import('/node_modules/@fontsource-variable/geist/index.css');
      const { createIsland } = await import('/src/pill/renderer.ts');
      const { emptyContext } = await import('/src/pill/contracts.ts');
      await document.fonts.ready;
      const failures = [];
      let checked = 0;
      for (const anchor of ['bottom-left', 'bottom-center', 'bottom-right', 'top-left', 'top-center', 'top-right']) {
        // Smart/expanded are identical here: both set show_start_card=true.
        for (const details of ['compact', 'expanded']) {
          for (const depth of [0, 1, 2, 3]) {
            const label = `${anchor}/${details}/depth=${depth}`;
            let regions = [], actionRects = [];
            const calls = [];
            let failRetry = false, holdRetry = false, finishRetry;
            const host = document.createElement('div');
            document.body.append(host);
            const island = createIsland(host, {
              staticPreview: true, mac: true, icon: async () => null,
              actions: { start: async () => {}, stop: async () => {}, cancel: async () => {}, dismiss: async () => {}, card: async c => { calls.push(c); if(holdRetry) await new Promise(resolve => { finishRetry = resolve; }); if(failRetry) throw 'Requested local engine is not ready'; } },
              hitRegions: (value, actions) => { regions = value; actionRects = actions; },
              setTimeout: window.setTimeout.bind(window), clearTimeout: window.clearTimeout.bind(window),
            });
            const check = (condition, message) => { if (!condition) failures.push(`${label}: ${message}`); };
            const verify = phase => {
              island.renderStatic();
              for (const card of host.querySelectorAll('.feedback-card')) {
                if (card.hidden || card.classList.contains('back')) continue;
                check(!card.closest('[inert]'), `${phase}: front card is inert`);
                const box = card.getBoundingClientRect();
                check(regions.some(r => Math.abs(r.x - box.x) < .1 && Math.abs(r.y - box.y) < .1 && Math.abs(r.width - box.width) < .1 && Math.abs(r.height - box.height) < .1), `${phase}: region differs from rendered card`);
                const title = card.querySelector('.card-title').getBoundingClientRect();
                check(!actionRects.some(r => title.x + title.width / 2 >= r.x && title.x + title.width / 2 <= r.x + r.width && title.y + title.height / 2 >= r.y && title.y + title.height / 2 <= r.y + r.height), `${phase}: text has a hand cursor region`);
                for (const button of card.querySelectorAll('button')) {
                  const b = button.getBoundingClientRect();
                  const cursorTarget = actionRects.some(r => Math.abs(r.x - b.x) < .1 && Math.abs(r.y - b.y) < .1 && Math.abs(r.width - b.width) < .1 && Math.abs(r.height - b.height) < .1);
                  check(cursorTarget === !button.disabled, `${phase}: enabled/disabled cursor region mismatch`);
                  check(getComputedStyle(button).cursor === (button.disabled ? 'default' : 'pointer'), `${phase}: incorrect enabled/disabled CSS cursor`);
                  for (const [x, y] of [[b.x + 1, b.y + 1], [b.right - 1, b.bottom - 1], [b.x + b.width / 2, b.y + b.height / 2]]) {
                    check(regions.some(r => x >= r.x && x <= r.x + r.width && y >= r.y && y <= r.y + r.height), `${phase}: action outside hit regions`);
                  }
                  // Rounded corners are part of the native region, but not the button's CSS hit shape.
                  check(button.contains(document.elementFromPoint(b.x + b.width / 2, b.y + b.height / 2)), `${phase}: action blocked in DOM`);
                  checked++;
                }
              }
              for (const r of actionRects) {
                check([...host.querySelectorAll('button')].some(button => {
                  if (button.disabled || button.closest('[hidden],[inert],.back')) return false;
                  const b = button.getBoundingClientRect();
                  return Math.abs(r.x - b.x) < .1 && Math.abs(r.y - b.y) < .1 && Math.abs(r.width - b.width) < .1 && Math.abs(r.height - b.height) < .1;
                }), `${phase}: non-action/back-card cursor region`);
              }
            };
            try {
              island.settings({ pill_indicator_mode: 'always' });
              island.geometry({ anchor, anchorX: anchor.endsWith('-left') ? 11 : anchor.endsWith('-right') ? 451 : 231, anchorY: anchor.startsWith('top-') ? 17 : 425 });
              island.context({ ...emptyContext(), generation: 1, show_start_card: details !== 'compact', changed_since_last: true });
              island.start(); island.renderStatic();
              island.work('transcribing'); island.renderStatic();
              island.recovery({ generation: 1, id: 'clip', kind: 'cloud_failed', engine_short: 'Soniox', alt_engine_short: 'Parakeet', alt_engine_id: 'parakeet', expires_in_ms: 600000 });
              verify('handoff');
              for (let i = 0; i < depth; i++) {
                island.feedback({ key: `cover${i}`, title: `Cover ${i}`, sub: '', tone: 'neutral', duration: null, actions: [] }, false);
                verify(`stacked ${i}`);
              }
              for (let i = depth - 1; i >= 0; i--) {
                host.querySelector(`.feedback-card[data-key="cover${i}"] .card-dismiss`).click();
                verify(`promoted ${i}`);
              }
              const recovery = host.querySelector('[data-key="recovery:clip"]');
              check(recovery.querySelectorAll('button').length === 2 && recovery.querySelector('[aria-label="Close, recording kept"]') && !recovery.textContent.includes('Discard'), 'expected Retry + non-destructive X');
              holdRetry = true;
              recovery.querySelector('.act').click();
              verify('pending retry excludes only Retry');
              check(recovery.querySelector('.act').disabled, 'pending retry not disabled');
              finishRetry(); holdRetry = false;
              await new Promise(resolve => setTimeout(resolve, 0));
              check(calls[0]?.command === 'retry_kept_dictation', 'Retry did not dispatch');
              island.recovery({ generation: 1, id: 'kept', kind: 'integrity', engine_short: 'Parakeet', alt_engine_short: 'Parakeet', alt_engine_id: 'parakeet', expires_in_ms: 600000, retry_ready: true });
              verify('kept after retry');
              host.querySelector('[data-key="recovery:kept"] .card-dismiss').click();
              island.renderStatic();
              check(calls.length === 1, 'X invoked audio deletion');
              failRetry = true;
              island.recovery({ generation: 1, id: 'failed', kind: 'integrity', engine_short: 'Parakeet', alt_engine_short: 'Parakeet', alt_engine_id: 'parakeet', expires_in_ms: 600000, retry_ready: true });
              host.querySelector('[data-key="recovery:failed"] .act').click();
              await Promise.resolve(); await Promise.resolve();
              verify('retry failure');
              const error = host.querySelector('[data-key="retry-error"]');
              check(error && !error.hidden && error.textContent.includes('That engine is no longer ready'), 'Retry failure was silent');
              error?.querySelector('.card-dismiss').click();
              verify('retry failure dismissed');
              island.recovery({ generation: 1, id: 'finishing', kind: 'integrity', engine_short: 'Parakeet', alt_engine_short: 'Parakeet', alt_engine_id: 'parakeet', expires_in_ms: 600000, retry_ready: false });
              verify('finishing audio');
              check(!host.querySelector('[data-key="recovery:finishing"] .act'), 'unfinished audio offers Retry');
              island.settings({ pill_indicator_mode: 'never' }); island.renderStatic();
              check(regions.length === 0 && actionRects.length === 0, 'hidden island retained native regions');
              island.settings({ pill_indicator_mode: 'always' });
              verify('shown again');
            } finally {
              island.destroy(); host.remove();
            }
          }
        }
      }
      return { failures, checked };
    });
    assert.equal(result.failures.length, 0, `display scale ${scale}: ${result.failures.slice(0, 12).join('; ')}`);
    checked += result.checked;
    await page.close();
  }
  console.log(`PASS: ${checked} rendered card actions; 96 anchor/start-detail/stack/scale cases.`);
} finally {
  await browser?.close();
  await server.close();
}
