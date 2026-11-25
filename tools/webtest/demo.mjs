export const sound = "bugs4m";
export async function actions({ page, check, report, shot }) {
  check((await report()).startsWith("calm, clouds"), `begins: ${await report()}`);
  await page.waitForFunction(() => parseFloat(window.demo.player.report().split("length ")[1]) >= 0, null, { timeout: 15000 });
  check(parseInt((await report()).split(", ")[2]) > 500, `blades: ${await report()}`);
  await shot("grown");
  await page.click('#modes [data-key=" "]');
  check((await report()).startsWith("storm") && (await report()).includes("wind 0.80"), `storm: ${await report()}`);
  await page.keyboard.press("x");
  check((await report()).includes("plasma"), `sky: ${await report()}`);
  const before = await report();
  await page.mouse.move(512, 380); await page.mouse.down(); await page.mouse.move(540, 385); await page.waitForTimeout(1500); await page.mouse.up();
  check(before.split("eye")[1] !== (await report()).split("eye")[1], `flies: ${await report()}`);
  await shot("storm");
  await page.keyboard.press("w"); await page.waitForTimeout(300); await shot("wires"); await page.keyboard.press("w");
  await page.keyboard.press("x"); await page.keyboard.press("x");
}
