export const sound = null;
export async function actions({ page, check, report, shot }) {
  // The report counts the real ATB assets that were loaded.
  check((await report()).includes("dict 38"), `dict: ${await report()}`);
  check((await report()).includes("meshes 7"), `meshes: ${await report()}`);
  // w toggles wireframe; the mode button does the same.
  await page.keyboard.press("w");
  check((await report()).includes("wire true"), `wire key: ${await report()}`);
  await page.click('#modes [data-key="w"]');
  check((await report()).includes("wire false"), `wire button: ${await report()}`);
  // p pauses the spin (still renders).
  await page.keyboard.press("p");
  await page.mouse.move(400, 400); await page.mouse.down(); await page.mouse.move(600, 420, { steps: 6 }); await page.mouse.up();
  await page.mouse.wheel(0, -150);
  await page.waitForTimeout(500);
  await shot("background");
}
