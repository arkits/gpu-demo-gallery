export const sound = null;
export async function actions({ page, check, report, shot }) {
  check((await report()).includes("anubis 0"), `idle: ${await report()}`);
  await page.keyboard.down("8"); await page.waitForTimeout(50); await page.keyboard.up("8");
  check((await report()).includes("anubis 6"), `punch: ${await report()}`);
  // The punch reaches her from where they start: she reels.
  await page.waitForTimeout(400);
  check((await report()).includes("nefertiti 5"), `punch lands: ${await report()}`);
  await page.waitForTimeout(1500);
  // A click presses and lets go between ticks; the kick must still start.
  await page.click('#modes [data-key="o"]');
  await page.waitForTimeout(100);
  check((await report()).includes("nefertiti 2"), `button kick: ${await report()}`);
  // Holding the Block button holds the block.
  await page.waitForTimeout(1500);
  const block = page.locator('#modes [data-key="u"]');
  await block.hover();
  await page.mouse.down();
  await page.waitForTimeout(400);
  check((await report()).includes("nefertiti 11"), `button block held: ${await report()}`);
  await page.mouse.up();
  await page.waitForTimeout(600);
  await shot("background");
}
