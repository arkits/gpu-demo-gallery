export const sound = null;
export async function actions({ page, check, report, shot }) {
  // The room draws from demoroom.bof: seven parts, 284 triangles a pass.
  check((await report()).includes("tris 284"), `room: ${await report()}`);
  // Space steps the projector's slides; the button does the same.
  await page.keyboard.press(" ");
  check((await report()).includes("slide 1"), `slide key: ${await report()}`);
  await page.click('#modes [data-key=" "]');
  check((await report()).includes("slide 2"), `slide button: ${await report()}`);
  await page.keyboard.press("f");
  check((await report()).includes("fog true"), `fog: ${await report()}`);
  await page.keyboard.down("ArrowLeft"); await page.waitForTimeout(300); await page.keyboard.up("ArrowLeft");
  await page.mouse.move(400, 400); await page.mouse.down(); await page.mouse.move(600, 420, { steps: 6 }); await page.mouse.up();
  await page.mouse.wheel(0, -150);
  await page.waitForTimeout(500);
  await shot("background");
}
