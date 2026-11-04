import { afterEach, expect, it, vi } from 'vitest';
import { enabledActionRects } from '@/pill/action-regions';

const surface = { x: 10, y: 10, width: 200, height: 80 };
afterEach(() => { document.body.replaceChildren(); vi.restoreAllMocks(); });
function fixture() {
  const root = document.createElement('div');
  root.innerHTML = '<div><span>Card text and empty space</span><button>Retry</button><button disabled>Pending</button></div><div class="back"><button>Back</button></div><div hidden><button>Hidden</button></div><div inert><button>Inert</button></div>';
  document.body.append(root);
  vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue(new DOMRect(100, 40, 60, 24));
  return root;
}
it('publishes only enabled action buttons, not card/text/back/hidden/inert regions', () => {
  expect(enabledActionRects(fixture(), [surface])).toEqual([{ x: 100, y: 40, width: 60, height: 24 }]);
});
it('does not duplicate a front button where broad back-card regions overlap', () => {
  expect(enabledActionRects(fixture(), [surface, { x: 130, y: 50, width: 200, height: 80 }])).toEqual([{ x: 100, y: 40, width: 60, height: 24 }]);
});
it('withdraws pending Retry while leaving X available, then restores Retry', () => {
  const root = fixture(), retry = root.querySelector('button')!;
  const close = document.createElement('button'); close.textContent = 'X'; root.append(close);
  close.getBoundingClientRect = () => new DOMRect(175, 40, 24, 24);
  retry.disabled = true;
  expect(enabledActionRects(root, [surface])).toEqual([{ x: 175, y: 40, width: 24, height: 24 }]);
  retry.disabled = false;
  expect(enabledActionRects(root, [surface])).toHaveLength(2);
});
it('clips to click-through surfaces and clears on hide/empty regions', () => {
  const root = fixture();
  expect(enabledActionRects(root, [{ x: 130, y: 50, width: 200, height: 80 }])).toEqual([{ x: 130, y: 50, width: 30, height: 14 }]);
  expect(enabledActionRects(root, [])).toEqual([]);
  root.hidden = true;
  expect(enabledActionRects(root, [surface])).toEqual([]);
});
it('intersects every scrolling ancestor and drops fully clipped picker rows', () => {
  const root = document.createElement('div');
  root.innerHTML = '<div style="overflow-x:hidden"><div class="pick-list" style="overflow-y:auto"><button>Partial</button><button>Outside</button></div></div>';
  document.body.append(root);
  root.firstElementChild!.getBoundingClientRect = () => new DOMRect(70, 10, 80, 90);
  root.querySelector('.pick-list')!.getBoundingClientRect = () => new DOMRect(60, 40, 120, 40);
  const buttons = root.querySelectorAll('button');
  buttons[0].getBoundingClientRect = () => new DOMRect(50, 30, 130, 60);
  buttons[1].getBoundingClientRect = () => new DOMRect(50, 5, 130, 20);
  expect(enabledActionRects(root, [surface])).toEqual([{ x: 70, y: 40, width: 80, height: 40 }]);
});
it('excludes transparent outgoing layers and noninteractive ghosts', () => {
  const root = fixture(), retry = root.querySelector('button')!;
  retry.parentElement!.style.opacity = '0';
  expect(enabledActionRects(root, [surface])).toEqual([]);
  retry.parentElement!.style.opacity = '1';
  retry.style.pointerEvents = 'none';
  expect(enabledActionRects(root, [surface])).toEqual([]);
});
