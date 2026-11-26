import { chromium, firefox } from './dependencies.mjs';
import { preview } from "../tools/assets/test-preview.mjs";
import { verifyStartupFailures, verifyStartupTimeout, verifyDecorativeFonts } from "./startup-failures.mjs";

const server = await preview();
try {
  for (const engine of [chromium, firefox]) {
    const browser = await engine.launch();
    try {
      const url = new URL("?lang=en", server.url.href).href;
      const prepare = page => page.route("**/api/**", route => route.fulfill({
        status: 401, contentType: "application/problem+json",
        body: JSON.stringify({ status: 401, code: "auth.required" }),
      }));
      await verifyDecorativeFonts(browser, url, prepare);
      await verifyStartupFailures(browser, url, prepare);
      await verifyStartupTimeout(browser, url, prepare);
    } finally { await browser.close(); }
  }
} finally { await server.close(); }
