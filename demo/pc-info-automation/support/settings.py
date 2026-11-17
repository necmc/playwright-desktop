from dataclasses import dataclass
import os
from pathlib import Path
from urllib.parse import unquote
from xml.etree import ElementTree


AUTOMATION_ROOT = Path(__file__).resolve().parents[1]
PROJECT_ROOT = AUTOMATION_ROOT.parents[1]
PUBLICATION = PROJECT_ROOT / "demo/pc-info/launcher-clickonce/publish"


@dataclass(frozen=True)
class Settings:
    java_home: Path
    activation_timeout: int
    port: int = 18765

    @property
    def base_url(self) -> str:
        return f"http://127.0.0.1:{self.port}"

    @classmethod
    def load(cls, *, java_home: Path | None = None, timeout: int = 25) -> "Settings":
        configured_home = java_home or published_java_home() or os.environ.get("JAVA_HOME")
        if not configured_home:
            raise RuntimeError("No se encontró OpenJDK. Compila la demo o indica --java-home 'C:/ruta/openjdk'.")
        return cls(java_home=Path(configured_home), activation_timeout=timeout * 1000)

    def validate(self) -> None:
        if os.name != "nt":
            raise RuntimeError("Estas pruebas requieren Windows.")
        if not (self.java_home / "bin/java.exe").is_file():
            raise RuntimeError(f"No se encontró Java en {self.java_home}.")
        if not (PUBLICATION / "InformationDemo.application").is_file():
            raise RuntimeError("Compila la demo antes de ejecutar sus pruebas.")


def published_java_home() -> str | None:
    """Usar el mismo runtime que el lanzador ClickOnce de la publicación vigente."""
    manifest = PUBLICATION / "InformationDemo.application"
    if not manifest.is_file():
        return None
    try:
        deployment = ElementTree.parse(manifest)
        application = deployment.find(".//{*}dependentAssembly")
        config = (PUBLICATION / unquote(application.attrib["codebase"])).with_name("InformationDemo.exe.config")
        setting = ElementTree.parse(config).find(".//add[@key='JavaHome']")
        return setting.attrib["value"]
    except (OSError, ElementTree.ParseError, AttributeError, KeyError) as error:
        raise RuntimeError("La publicación ClickOnce está incompleta; vuelve a compilar la demo.") from error
