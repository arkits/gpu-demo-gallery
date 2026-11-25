export const sound = null;
export async function actions({ page, check, report, shot }) {
  // A camera preset changes the reported camera number.
  await page.keyboard.press("2");
  check((await report()).includes("camera 2"), `camera key: ${await report()}`);
  await page.click('#modes [data-key="2"]');
  check((await report()).includes("camera 2"), `camera button: ${await report()}`);
  // r toggles the right screen between 30 Hz and 15 Hz.
  await page.keyboard.press("r");
  check((await report()).includes("15Hz"), `rate: ${await report()}`);
  await page.keyboard.press("r");
  check((await report()).includes("30Hz"), `rate back: ${await report()}`);
  // space and b toggle the spinning scene and the bouncing ball.
  await page.keyboard.press(" ");
  check((await report()).includes("spin false"), `spin: ${await report()}`);
  await page.keyboard.press("b");
  check((await report()).includes("bounce false"), `bounce: ${await report()}`);
  await page.keyboard.press("4");
  check((await report()).includes("camera 4"), `camera 4: ${await report()}`);
  await page.mouse.move(400, 400); await page.mouse.down(); await page.mouse.move(600, 420, { steps: 6 }); await page.mouse.up();
  await page.mouse.wheel(0, -150);
  await page.waitForTimeout(500);
  await shot("background");
}
