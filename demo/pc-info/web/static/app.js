import { openDesktopApplication } from "./desktop.js";

const API_OPERATIONS_PATH = "/api/operations";
const POLL_INTERVAL_MS = 1000;
const RESULT_FIELDS = ["date", "computerName", "alias"];
const STATUS = {
  COMPLETED: "completed",
  CANCELLED: "cancelled",
  FAILED: "failed",
};
const SELECTORS = {
  FORM: "#information-form",
  OBTAIN_BUTTON: "#obtain-button",
  OPERATION_STATUS: "#operation-status",
  LAUNCH_LINK: "#launch-link",
  PDF_VIEWER: "#pdf-viewer",
  PREVIEW_PLACEHOLDER: "#preview-placeholder",
  PDF_LINK: "#pdf-link",
};

const form = document.querySelector(SELECTORS.FORM);
const button = document.querySelector(SELECTORS.OBTAIN_BUTTON);
const status = document.querySelector(SELECTORS.OPERATION_STATUS);
const launchLink = document.querySelector(SELECTORS.LAUNCH_LINK);
const pdfViewer = document.querySelector(SELECTORS.PDF_VIEWER);
const previewPlaceholder = document.querySelector(SELECTORS.PREVIEW_PLACEHOLDER);
const pdfLink = document.querySelector(SELECTORS.PDF_LINK);
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
    const response = await fetch(`${API_OPERATIONS_PATH}/${current}`);
    const result = await response.json();
    if (operationId !== current) return;
    if (!response.ok) throw new Error(result.error || "No se pudo consultar la operación.");
    if (result.status === STATUS.COMPLETED) {
      for (const name of RESULT_FIELDS) form.elements[name].value = result.data[name];
      pdfViewer.src = result.pdf_url;
      pdfViewer.hidden = false;
      previewPlaceholder.hidden = true;
      pdfLink.href = result.pdf_url;
      pdfLink.hidden = false;
      showStatus("Información recibida. El PDF está listo.");
      finish();
    } else if (result.status === STATUS.CANCELLED) {
      showStatus("La operación se canceló desde la aplicación.");
      finish();
    } else if (result.status === STATUS.FAILED) {
      throw new Error(result.error || "No se pudo abrir la aplicación de escritorio.");
    } else {
      pollTimer = window.setTimeout(pollOperation, POLL_INTERVAL_MS);
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
    const response = await fetch(API_OPERATIONS_PATH, { method: "POST" });
    const result = await response.json();
    if (!response.ok) throw new Error(result.error || "No se pudo iniciar la operación.");
    operationId = result.operation_id;
    form.reset();
    pdfViewer.hidden = true;
    pdfViewer.removeAttribute("src");
    previewPlaceholder.hidden = false;
    pdfLink.hidden = true;
    launchLink.href = result.launch_url;
    launchLink.hidden = false;
    openDesktopApplication(result.launch_url);
    showStatus("Esperando la aplicación. Si el navegador solicita abrirla, confirma la apertura. Después revisa los datos y pulsa Confirmar.");
    pollTimer = window.setTimeout(pollOperation, POLL_INTERVAL_MS);
  } catch (error) {
    showStatus(error.message, true);
    finish();
  }
});

window.addEventListener("pagehide", () => window.clearTimeout(pollTimer));
