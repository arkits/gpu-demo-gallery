export const sound = null;
export async function actions({ page, check, report, shot }) {
  check((await report()).includes("a_moves.3ds"), `first model: ${await report()}`);
  await page.keyboard.press("2");
  check((await report()).includes("atat.3ds"), `model key: ${await report()}`);
  await page.click('#modes [data-key="6"]');
  check((await report()).includes("martini.vq"), `model button: ${await report()}`);
  await page.keyboard.down("ArrowLeft"); await page.waitForTimeout(300); await page.keyboard.up("ArrowLeft");
  check(!(await report()).includes("yaw 0.00"), `turns: ${await report()}`);
  await page.mouse.move(400, 400); await page.mouse.down(); await page.mouse.move(600, 420, { steps: 6 }); await page.mouse.up();
  await page.waitForTimeout(400);
  await shot("background");
}
