---
name: git-my-commit
description: "Crea commits de Git con Conventional Commits en el idioma de la conversación. Úsala cuando el usuario pida comitear o redactar un mensaje de commit."
---

# Git My Commit

Revisa el diff de los cambios que el usuario pidió incluir y describe el cambio principal con precisión. Si pide un mensaje, entrega el texto. Si pide comitear, crea el commit con los cambios autorizados.

- Usa `tipo(alcance): resumen` o `tipo: resumen`. Elige el tipo según el cambio (`feat`, `fix`, `docs`, `refactor`, `test`, `chore`, etc.). Para un cambio incompatible, añade `!` antes de `:`.
- Escribe el resumen y el cuerpo en el idioma que usas con el usuario, salvo que este indique otro.
- El título completo admite hasta 10 palabras. Cuenta los grupos separados por espacios, incluido el prefijo como un grupo.
- Usa solo el título por defecto. Si el usuario pide cuerpo, deja una línea en blanco después del título y escribe de uno a cuatro ítems consecutivos, cada uno en su propia línea con `- `. Cada ítem admite hasta 15 palabras sin contar la viñeta.
- Verifica el formato y los límites antes de entregar el mensaje o crear el commit. Para comitear con cuerpo, usa `git commit -F` con un archivo que preserve los saltos de línea.

[Especificación de Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/)
