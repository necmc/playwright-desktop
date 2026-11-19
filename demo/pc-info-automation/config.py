from pathlib import Path
from urllib.parse import unquote
from xml.etree import ElementTree


DEMO_URL = "http://127.0.0.1:18765"
ACTIVATION_TIMEOUT_MS = 25_000
PUBLICATION = Path(__file__).resolve().parents[1] / "pc-info/launcher-clickonce/publish"


def published_java_home() -> str | None:
    """Lee el runtime de ClickOnce para localizar su Java Access Bridge."""
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
