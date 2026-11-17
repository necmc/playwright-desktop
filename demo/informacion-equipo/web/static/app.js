import { openDesktopApplication } from "./desktop.js";

const form = document.querySelector("#information-form");
const button = document.querySelector("#obtain-button");
const status = document.querySelector("#operation-status");
const launchLink = document.querySelector("#launch-link");
let operationId = null;
let pollTimer = null;

function showStatus(message, error = false) {
  status.textContent = message;
  status.dataset.error = String(error);
}

function finish() {
  operationId = null;
  button.disabled = false;
  launchLink.hidden = true;
  window.clearTimeout(pollTimer);
}

async function pollOperation() {
  const current = operationId;
  try {
    const response = await fetch(`/api/operations/${current}`);
    const result = await response.json();
    if (operationId !== current) return;
    if (!response.ok) throw new Error(result.error || "No se pudo consultar la operación.");
    if (result.status === "completed") {
      for (const name of ["date", "computerName", "alias"]) form.elements[name].value = result.data[name];
      document.querySelector("#pdf-viewer").src = result.pdf_url;
      document.querySelector("#pdf-viewer").hidden = false;
      document.querySelector("#preview-placeholder").hidden = true;
      document.querySelector("#pdf-link").href = result.pdf_url;
      document.querySelector("#pdf-link").hidden = false;
      showStatus("Información recibida. El PDF está listo.");
      finish();
    } else if (result.status === "cancelled") {
      showStatus("La operación se canceló desde la aplicación.");
      finish();
    } else if (result.status === "failed") {
      throw new Error(result.error || "No se pudo abrir la aplicación de escritorio.");
    } else {
      pollTimer = window.setTimeout(pollOperation, 1000);
    }
  } catch (error) {
    if (operationId !== current) return;
    showStatus(error.message, true);
    finish();
  }
}

form.addEventListener("submit", async (event) => {
  event.preventDefault();
  button.disabled = true;
  showStatus("Preparando la aplicación de escritorio…");
  try {
    const response = await fetch("/api/operations", { method: "POST" });
    const result = await response.json();
    if (!response.ok) throw new Error(result.error || "No se pudo iniciar la operación.");
    operationId = result.operation_id;
    form.reset();
    document.querySelector("#pdf-viewer").hidden = true;
    document.querySelector("#pdf-viewer").removeAttribute("src");
    document.querySelector("#preview-placeholder").hidden = false;
    document.querySelector("#pdf-link").hidden = true;
    launchLink.href = result.launch_url;
    launchLink.hidden = false;
    openDesktopApplication(result.launch_url);
    showStatus("Esperando la aplicación. Si el navegador solicita abrirla, confirma la apertura. Después revisa los datos y pulsa Confirmar.");
    pollTimer = window.setTimeout(pollOperation, 1000);
  } catch (error) {
    showStatus(error.message, true);
    finish();
  }
});

window.addEventListener("pagehide", () => window.clearTimeout(pollTimer));
