/** Activación ClickOnce desde la página de la demo. */
export function openDesktopApplication(launchUrl) {
  const url = new URL(launchUrl, window.location.href);
  if (url.origin !== window.location.origin || !url.pathname.endsWith(".application")) {
    throw new Error("La dirección de la aplicación de escritorio es inválida.");
  }
  const frame = document.createElement("iframe");
  frame.hidden = true;
  frame.title = "Activación de la aplicación de escritorio";
  frame.src = url.href;
  document.body.append(frame);
  window.setTimeout(() => frame.remove(), 15000);
}
