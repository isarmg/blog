// Keep startup failures visible even when the Rust/WASM module cannot load.
// Successful application rendering remains owned by Rust.
export async function startApplication(initialize) {
  const html = document.documentElement;
  if (html.dataset.blogStartup) throw new Error("Application startup already began");
  html.dataset.blogStartup = "pending";
  const listeners = new AbortController();
  let timeout;
  const interrupted = new Promise(resolve => {
    const scriptError = event => resolve({ stage: "runtime", error: event.error ?? event.reason });
    window.addEventListener("error", scriptError, { signal: listeners.signal });
    window.addEventListener("unhandledrejection", scriptError, { signal: listeners.signal });
    timeout = setTimeout(() => resolve({ stage: "timeout" }), 30_000);
  });
  const initialized = Promise.resolve().then(initialize).then(
    () => null,
    error => ({ stage: "runtime", error }),
  );
  try {
    const failure = await Promise.race([initialized, interrupted]);
    if (failure) {
      html.dataset.blogStartup = "failed";
      delete html.dataset.blogRuntime;
      showStartupFailure(html.lang, failure);
    } else {
      html.dataset.blogStartup = "ready";
    }
  } finally {
    clearTimeout(timeout);
    listeners.abort();
  }
}

function showStartupFailure(defaultLanguage, { stage, error }) {
  const language = new URL(window.location.href).searchParams.get("lang") ?? defaultLanguage;
  const chinese = language.toLowerCase().startsWith("zh");
  const panel = document.createElement("section");
  panel.id = "blog-startup-error";
  panel.setAttribute("role", "alert");
  panel.setAttribute("aria-labelledby", "blog-startup-error-title");
  panel.dataset.stage = stage;
  panel.tabIndex = -1;
  const title = document.createElement("h1");
  title.id = "blog-startup-error-title";
  title.textContent = chinese ? "页面启动失败" : "Page could not start";
  const message = document.createElement("p");
  message.textContent = chinese
    ? "页面所需资源加载失败或超时。请检查网络连接，然后重新加载页面。"
    : "Required page resources failed to load or timed out. Check your connection, then reload the page.";
  const retry = document.createElement("button");
  retry.type = "button";
  retry.textContent = chinese ? "重新加载页面" : "Reload page";
  retry.addEventListener("click", () => {
    retry.disabled = true;
    window.location.reload();
  }, { once: true });
  panel.append(title, message, retry);
  document.body.append(panel);
  panel.focus({ preventScroll: true });
  console.error("Application startup failed", stage, error ?? "");
}
