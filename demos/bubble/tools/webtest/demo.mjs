export const sound = null;
export async function actions({ page, check, report, shot }) {
  check((await report()).startsWith("cube map, 5120"), `begins: ${await report()}`);
  await page.mouse.click(640, 360);
  check((await report()).endsWith("1 pokes"), `click: ${await report()}`);
  await page.keyboard.press("2");
  check((await report()).includes("320 triangles"), `coarse: ${await report()}`);
  await page.keyboard.press("s");
  check((await report()).startsWith("sphere map"), `sphere: ${await report()}`);
  await page.click('#modes [data-key="t"]');
  check((await report()).startsWith("sphere picture"), `picture: ${await report()}`);
  await page.waitForTimeout(400);
  await shot("picture");
  await page.keyboard.press("t"); await page.keyboard.press("c"); await page.keyboard.press("4");
  await page.mouse.click(700, 330);
  await page.waitForTimeout(250);
  await shot("poked");
}
