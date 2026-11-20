"""Ejecuta el escenario funcional de la demo con su configuración local."""

from http.client import HTTPException
import sys
import unittest
from urllib.request import urlopen

sys.dont_write_bytecode = True

from config import DEMO_URL


def main() -> int:
    # El formulario identifica a pc-info aunque otro servicio también devuelva HTTP 200.
    try:
        with urlopen(DEMO_URL, timeout=5) as response:
            demo_ready = response.status == 200 and b'id="information-form"' in response.read(8192)
    except (OSError, HTTPException):
        demo_ready = False
    if not demo_ready:
        print("El sistema no responde.", file=sys.stderr)
        return 1

    try:
        from tests.test_flow import PcInfoFlowTests
    except ModuleNotFoundError as error:
        print(f"No se puede iniciar la automatización: {error}", file=sys.stderr)
        print("Consulta la preparación en demo/README.md.", file=sys.stderr)
        return 1

    suite = unittest.defaultTestLoader.loadTestsFromTestCase(PcInfoFlowTests)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    return 0 if result.wasSuccessful() and not result.skipped else 1


if __name__ == "__main__":
    raise SystemExit(main())
