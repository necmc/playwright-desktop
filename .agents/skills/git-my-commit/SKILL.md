---
name: git-my-commit
description: "Crea o revisa commits de Git siguiendo Conventional Commits en el idioma de la conversación. Úsala siempre que el usuario pida comitear cambios, redactar, revisar, corregir o enmendar un mensaje de commit, incluso si solo indica guardar los cambios en el repositorio."
---

# Git My Commit

Revisa el diff de los cambios que el usuario autorizó incluir y describe con precisión el aporte principal. Si pide un mensaje, entrega el texto; si pide comitear, crea el commit únicamente con los cambios autorizados.

## Reglas de redacción

- **Formato:** Usa `tipo(alcance): resumen` o `tipo: resumen` según el tipo de cambio (`feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `ci`, `build`). Añade `!` antes de `:` ante cambios incompatibles.
- **Idioma:** Escribe el título y el cuerpo en el idioma de la conversación, salvo que el usuario indique otro.
- **Enfoque funcional primero:** Describe la capacidad o el comportamiento entregado en lugar de nombres de funciones, variables o detalles internos, para que el historial comunique valor sin obligar a leer el diff. Usa términos técnicos únicamente cuando el cambio sea estrictamente técnico (p. ej., mitigación de una vulnerabilidad identificada como un CVE o corrección de compilación).
- **Sin versiones de liberación:** Omite versiones del proyecto (`0.2.0`, `v1.3.0`) en el título y cuerpo, incluso al editar manifiestos; correcciones previas al corte pueden mover la entrega final y la versión oficial la define solo el tag de Git.
- **Límites de longitud:** Mantén el título en un máximo de **10 palabras** (contando grupos separados por espacios, incluido el prefijo como un grupo). Por defecto usa solo título. Si el usuario pide cuerpo, deja una línea en blanco y escribe entre 1 y 4 viñetas con `- `, cada una de hasta **15 palabras** sin contar el guion.

## Ejemplos

**Enfoque funcional**
- Incorrecto: `feat(lib): exponer función os_language en DesktopContext`
- Correcto: `feat(lib): añadir detección automática del idioma del SO`

**Sin versiones de liberación**
- Incorrecto: `chore: centralizar versión del paquete y actualizar a 0.2.0`
- Correcto: `chore: centralizar fuente de versión y preparar liberación del paquete`

## Verificación antes de entregar o comitear

- Cuenta los grupos separados por espacios del título (<= 10) y de cada viñeta del cuerpo (<= 15).
- Confirma que no queden versiones de liberación del proyecto ni símbolos internos evitables.
- Si el commit incluye cuerpo, pásalo con `git commit -F` desde un archivo que preserve saltos de línea y elimina el archivo temporal al terminar.

[Especificación de Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/)
