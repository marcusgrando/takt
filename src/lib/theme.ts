export function initTheme() {
  const applyTheme = (dark: boolean) =>
    document.documentElement.classList.toggle("dark", dark);

  const mq = window.matchMedia("(prefers-color-scheme: dark)");
  applyTheme(mq.matches);
  mq.addEventListener("change", (e) => applyTheme(e.matches));
}
