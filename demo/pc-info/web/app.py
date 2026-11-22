"""Servidor de la demo propia web, ClickOnce, Java y PDF."""

from http import HTTPStatus
from io import BytesIO
import os
from pathlib import Path

from flask import Flask, jsonify, render_template, request, send_file, send_from_directory, url_for

from operations import (
    OperationError,
    OperationStore,
    STATUS_CANCELLED,
    STATUS_COMPLETED,
    STATUS_FAILED,
)

DEMO_DIRECTORY = Path(__file__).resolve().parents[1]
DEFAULT_HOST = "127.0.0.1"
DEFAULT_PORT = "18765"
DEMO_PORT_ENV_VAR = "DEMO_PORT"
MAX_REQUEST_BYTES = 64 * 1024
MAX_ERROR_MESSAGE_LENGTH = 500
TRUSTED_HOSTS = ["127.0.0.1", "localhost"]
PUBLISH_RELATIVE_PATH = Path("launcher-clickonce") / "publish"
INDEX_TEMPLATE = "index.html"
MANIFEST_FILENAME = "InformationDemo.application"
PDF_DOWNLOAD_NAME = "pc-info.pdf"
CACHE_CONTROL_NO_STORE = "no-store"
DEFAULT_CONTENT_TYPE = "application/octet-stream"
PDF_CONTENT_TYPE = "application/pdf"
CLICKONCE_CONTENT_TYPES = {
    ".application": "application/x-ms-application",
    ".manifest": "application/x-ms-manifest",
}


def create_app() -> Flask:
    app = Flask(__name__)
    app.config.update(
        TEMPLATES_AUTO_RELOAD=True,
        MAX_CONTENT_LENGTH=MAX_REQUEST_BYTES,
        TRUSTED_HOSTS=TRUSTED_HOSTS,
        PUBLISH_DIRECTORY=DEMO_DIRECTORY / PUBLISH_RELATIVE_PATH,
    )
    store = OperationStore()
    app.extensions["operations"] = store

    @app.after_request
    def prevent_caching(response):
        response.headers["Cache-Control"] = CACHE_CONTROL_NO_STORE
        return response

    @app.errorhandler(OperationError)
    def operation_error(error):
        return jsonify(error=str(error)), error.status_code

    @app.errorhandler(HTTPStatus.REQUEST_ENTITY_TOO_LARGE)
    def request_too_large(error):
        return (
            jsonify(error="La petición supera el límite de 64 KB."),
            HTTPStatus.REQUEST_ENTITY_TOO_LARGE,
        )

    @app.get("/")
    def index():
        return render_template(INDEX_TEMPLATE)

    @app.get("/clickonce/<path:filename>")
    def clickonce_file(filename):
        content_type = CLICKONCE_CONTENT_TYPES.get(Path(filename).suffix, DEFAULT_CONTENT_TYPE)
        return send_from_directory(app.config["PUBLISH_DIRECTORY"], filename, mimetype=content_type)

    @app.post("/api/operations")
    def create_operation():
        manifest = Path(app.config["PUBLISH_DIRECTORY"]) / MANIFEST_FILENAME
        if not manifest.is_file():
            return (
                jsonify(error="Compila la demo con demo/pc-info/launcher-clickonce/build.ps1 antes de abrir la aplicación."),
                HTTPStatus.SERVICE_UNAVAILABLE,
            )
        operation_id = store.create()
        operation_url = url_for("operation_status", operation_id=operation_id, _external=True)
        launch_url = url_for(
            "clickonce_file", filename=MANIFEST_FILENAME, operation=operation_url, _external=True
        )
        return jsonify(operation_id=operation_id, launch_url=launch_url), HTTPStatus.CREATED

    @app.get("/api/operations/<operation_id>")
    def operation_status(operation_id):
        result = store.snapshot(operation_id)
        if result["status"] == STATUS_COMPLETED:
            result["pdf_url"] = url_for("operation_document", operation_id=operation_id)
        return jsonify(result)

    @app.post("/api/operations/<operation_id>/result")
    def operation_result(operation_id):
        try:
            data = store.confirm(operation_id, request.form)
        except OperationError:
            raise
        except Exception:
            app.logger.exception("No se pudo generar el PDF de la operación.")
            return (
                jsonify(error="No se pudo generar el PDF. Puedes reintentar la confirmación."),
                HTTPStatus.INTERNAL_SERVER_ERROR,
            )
        return jsonify(status=STATUS_COMPLETED, data=data)

    @app.post("/api/operations/<operation_id>/cancel")
    def operation_cancel(operation_id):
        store.finish(operation_id, STATUS_CANCELLED)
        return jsonify(status=store.snapshot(operation_id)["status"])

    @app.post("/api/operations/<operation_id>/error")
    def operation_failure(operation_id):
        message = request.form.get("message", "No se pudo iniciar la aplicación Java.").strip()[:MAX_ERROR_MESSAGE_LENGTH]
        store.finish(operation_id, STATUS_FAILED, message)
        return jsonify(status=store.snapshot(operation_id)["status"])

    @app.get("/api/operations/<operation_id>/document.pdf")
    def operation_document(operation_id):
        return send_file(
            BytesIO(store.document(operation_id)),
            mimetype=PDF_CONTENT_TYPE,
            download_name=PDF_DOWNLOAD_NAME,
        )

    return app


if __name__ == "__main__":
    create_app().run(
        host=DEFAULT_HOST,
        port=int(os.environ.get(DEMO_PORT_ENV_VAR, DEFAULT_PORT)),
        debug=False,
        use_reloader=True,
    )
