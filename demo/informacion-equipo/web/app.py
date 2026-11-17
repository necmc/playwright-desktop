"""Servidor de la demo propia web, ClickOnce, Java y PDF."""

from io import BytesIO
import os
from pathlib import Path

from flask import Flask, jsonify, render_template, request, send_file, send_from_directory, url_for

from operations import OperationError, OperationStore


DEMO_DIRECTORY = Path(__file__).resolve().parents[1]


def create_app() -> Flask:
    app = Flask(__name__)
    app.config.update(
        TEMPLATES_AUTO_RELOAD=True,
        MAX_CONTENT_LENGTH=64 * 1024,
        TRUSTED_HOSTS=["127.0.0.1", "localhost"],
        PUBLISH_DIRECTORY=DEMO_DIRECTORY / "launcher-clickonce" / "publish",
    )
    store = OperationStore()
    app.extensions["operations"] = store

    @app.after_request
    def prevent_caching(response):
        response.headers["Cache-Control"] = "no-store"
        return response

    @app.errorhandler(OperationError)
    def operation_error(error):
        return jsonify(error=str(error)), error.status_code

    @app.errorhandler(413)
    def request_too_large(error):
        return jsonify(error="La petición supera el límite de 64 KB."), 413

    @app.get("/")
    def index():
        return render_template("index.html")

    @app.get("/clickonce/<path:filename>")
    def clickonce_file(filename):
        content_type = "application/octet-stream"
        if filename.endswith(".application"):
            content_type = "application/x-ms-application"
        elif filename.endswith(".manifest"):
            content_type = "application/x-ms-manifest"
        return send_from_directory(app.config["PUBLISH_DIRECTORY"], filename, mimetype=content_type)

    @app.post("/api/operations")
    def create_operation():
        manifest = Path(app.config["PUBLISH_DIRECTORY"]) / "InformationDemo.application"
        if not manifest.is_file():
            return jsonify(error="Compila la demo con demo/informacion-equipo/launcher-clickonce/build.ps1 antes de abrir la aplicación."), 503
        operation_id = store.create()
        operation_url = url_for("operation_status", operation_id=operation_id, _external=True)
        launch_url = url_for(
            "clickonce_file", filename="InformationDemo.application", operation=operation_url, _external=True
        )
        return jsonify(operation_id=operation_id, launch_url=launch_url), 201

    @app.get("/api/operations/<operation_id>")
    def operation_status(operation_id):
        result = store.snapshot(operation_id)
        if result["status"] == "completed":
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
            return jsonify(error="No se pudo generar el PDF. Puedes reintentar la confirmación."), 500
        return jsonify(status="completed", data=data)

    @app.post("/api/operations/<operation_id>/cancel")
    def operation_cancel(operation_id):
        store.finish(operation_id, "cancelled")
        return jsonify(status=store.snapshot(operation_id)["status"])

    @app.post("/api/operations/<operation_id>/error")
    def operation_failure(operation_id):
        message = request.form.get("message", "No se pudo iniciar la aplicación Java.").strip()[:500]
        store.finish(operation_id, "failed", message)
        return jsonify(status=store.snapshot(operation_id)["status"])

    @app.get("/api/operations/<operation_id>/document.pdf")
    def operation_document(operation_id):
        return send_file(
            BytesIO(store.document(operation_id)), mimetype="application/pdf",
            download_name="informacion-equipo.pdf",
        )

    return app


if __name__ == "__main__":
    create_app().run(host="127.0.0.1", port=int(os.environ.get("DEMO_PORT", "18765")), debug=False, use_reloader=True)
