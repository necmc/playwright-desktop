"""Ejecuta el escenario funcional de la demo con su configuración local."""

import argparse
from pathlib import Path
import sys
import unittest

sys.dont_write_bytecode = True

from support.settings import Settings


def main() -> int:
    parser = argparse.ArgumentParser(description="Automatiza la demo web, Java y PDF con POM.")
    parser.add_argument("--java-home", type=Path, help="Ruta de OpenJDK; se detecta desde la demo compilada.")
    parser.add_argument("--timeout", type=int, default=25, help="Plazo de apertura de Java en segundos (25 por defecto).")
    args = parser.parse_args()
    if args.timeout <= 0:
        parser.error("--timeout debe ser mayor que cero.")

    try:
        settings = Settings.load(java_home=args.java_home, timeout=args.timeout)
        settings.validate()
        from tests.test_information_flow import InformationFlowTests
    except (RuntimeError, ModuleNotFoundError) as error:
        print(f"No se puede iniciar la automatización: {error}", file=sys.stderr)
        print("Consulta la preparación en demo/README.md.", file=sys.stderr)
        return 1

    InformationFlowTests.settings = settings
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(InformationFlowTests)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
