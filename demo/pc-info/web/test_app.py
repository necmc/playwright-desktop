"""Contratos de la demo y consistencia del resultado confirmado."""

from concurrent.futures import ThreadPoolExecutor
from io import BytesIO
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from pypdf import PdfReader

from app import create_app
from operations import OperationError


class OperationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.app = create_app()
        self.app.config.update(TESTING=True, PUBLISH_DIRECTORY=Path(self.temporary.name))
        self.client = self.app.test_client()
        Path(self.temporary.name, "InformationDemo.application").write_text("test manifest", encoding="utf-8")
        self.store = self.app.extensions["operations"]
        self.data = {"date": "2026-11-17", "computerName": "EQUIPO-QA", "alias": "Álvaro Ñ <demo> & QA"}

    def operation(self):
        response = self.client.post("/api/operations")
        self.assertEqual(response.status_code, 201)
        return response.get_json()["operation_id"]

    def test_initial_page_renders_only_form_and_hides_pdf_section(self):
        response = self.client.get("/")
        self.assertEqual(response.status_code, 200)
        html = response.get_data(as_text=True)
        self.assertIn('id="information-section"', html)
        self.assertNotIn('id="information-section" class="information" hidden', html)
        self.assertIn('id="preview-section"', html)
        self.assertIn('aria-label="Vista previa del PDF" hidden', html)
        self.assertIn("Obtener información", html)
        self.assertNotIn("preview-placeholder", html)

    def test_launcher_reference_and_manifest_content_type(self):
        response = self.client.post("/api/operations")
        result = response.get_json()
        self.assertIn("/clickonce/InformationDemo.application?operation=", result["launch_url"])
        self.assertIn(result["operation_id"], result["launch_url"])
        with self.client.get("/clickonce/InformationDemo.application") as manifest:
            self.assertEqual(manifest.content_type, "application/x-ms-application")

    def test_missing_publication_has_an_actionable_error(self):
        Path(self.temporary.name, "InformationDemo.application").unlink()
        self.assertEqual(self.client.post("/api/operations").status_code, 503)

    def test_confirmation_isolated_and_pdf_contains_confirmed_values(self):
        first, second = self.operation(), self.operation()
        response = self.client.post(f"/api/operations/{first}/result", data=self.data)
        self.assertEqual(response.status_code, 200)
        result = self.client.get(f"/api/operations/{first}").get_json()
        self.assertEqual(result["data"], self.data)
        self.assertEqual(self.client.get(f"/api/operations/{second}").get_json()["status"], "pending")
        pdf = self.client.get(result["pdf_url"])
        self.assertEqual(pdf.content_type, "application/pdf")
        text = "\n".join(page.extract_text() for page in PdfReader(BytesIO(pdf.data)).pages)
        for value in self.data.values():
            self.assertIn(value, text)

    def test_retry_is_idempotent_and_conflicting_confirmation_is_rejected(self):
        operation_id = self.operation()
        route = f"/api/operations/{operation_id}/result"
        self.assertEqual(self.client.post(route, data=self.data).status_code, 200)
        original = self.store.document(operation_id)
        self.assertEqual(self.client.post(route, data=self.data).status_code, 200)
        self.assertEqual(self.store.document(operation_id), original)
        self.assertEqual(self.client.post(route, data={**self.data, "alias": "Otro"}).status_code, 409)

    def test_parallel_confirmations_preserve_a_single_result(self):
        operation_id = self.operation()

        def confirm(alias):
            try:
                self.store.confirm(operation_id, {**self.data, "alias": alias})
                return 200
            except OperationError as error:
                return error.status_code

        with ThreadPoolExecutor(max_workers=2) as executor:
            statuses = list(executor.map(confirm, ("Uno", "Dos")))
        self.assertEqual(sorted(statuses), [200, 409])
        self.assertIn(self.store.snapshot(operation_id)["data"]["alias"], ("Uno", "Dos"))

    def test_validation_keeps_the_operation_available_for_correction(self):
        operation_id = self.operation()
        route = f"/api/operations/{operation_id}/result"
        for invalid in ({"date": "2026-02-30"}, {"date": "20261117"}, {"alias": " "}, {"computerName": "x" * 101}):
            self.assertEqual(self.client.post(route, data={**self.data, **invalid}).status_code, 422)
            self.assertEqual(self.store.snapshot(operation_id)["status"], "pending")
        self.assertEqual(self.client.post(route, data=self.data).status_code, 200)

    def test_pdf_failure_does_not_commit_a_partial_confirmation(self):
        operation_id = self.operation()
        with patch("operations.create_report", side_effect=OSError("test failure")), self.assertLogs(self.app.logger, level="ERROR"):
            self.assertEqual(self.client.post(f"/api/operations/{operation_id}/result", data=self.data).status_code, 500)
        self.assertEqual(self.store.snapshot(operation_id)["status"], "pending")
        self.assertEqual(self.client.post(f"/api/operations/{operation_id}/result", data=self.data).status_code, 200)

    def test_cancel_expiry_and_startup_error_are_reported(self):
        cancelled = self.operation()
        self.assertEqual(self.client.post(f"/api/operations/{cancelled}/cancel").status_code, 200)
        self.assertEqual(self.client.post(f"/api/operations/{cancelled}/result", data=self.data).status_code, 409)
        failed = self.operation()
        self.client.post(f"/api/operations/{failed}/error", data={"message": "Java no disponible"})
        self.assertEqual(self.store.snapshot(failed)["error"], "Java no disponible")
        expired = self.operation()
        with patch("operations.monotonic", return_value=10**12):
            self.assertEqual(self.client.get(f"/api/operations/{expired}").status_code, 410)
        self.assertEqual(self.client.get("/api/operations/unknown").status_code, 404)


if __name__ == "__main__":
    unittest.main()
