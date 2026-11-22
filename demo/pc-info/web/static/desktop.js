/** Activación ClickOnce desde la página de la demo. */
const MANIFEST_EXTENSION = ".application";
const FRAME_TITLE = "Activación de la aplicación de escritorio";
const FRAME_CLEANUP_DELAY_MS = 15000;

export function openDesktopApplication(launchUrl) {
  const url = new URL(launchUrl, window.location.href);
  if (url.origin !== window.location.origin || !url.pathname.endsWith(MANIFEST_EXTENSION)) {
    throw new Error("La dirección de la aplicación de escritorio es inválida.");
  }
  const frame = document.createElement("iframe");
  frame.hidden = true;
  frame.title = FRAME_TITLE;
  frame.src = url.href;
  document.body.append(frame);
  window.setTimeout(() => frame.remove(), FRAME_CLEANUP_DELAY_MS);
}
