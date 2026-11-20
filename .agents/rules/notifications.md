---
trigger: always_on
---

# Regla de Notificaciones Push (Telegram)

Debes utilizar el script de notificaciones ubicado en `tools/notify.py` para alertar al usuario en su celular bajo **estrictamente las siguientes situaciones**:

1. **Permisos:** Cuando se requiera autorización del usuario para continuar con una acción.
2. **Intervención humana:** Cuando necesites una respuesta, aclaración o entrada de datos del usuario para avanzar.
3. **Progreso y Cierre:** Al culminar tareas o reportar hitos significativos de avance en tareas extensas (con una duración estimada mayor a 2 minutos).
4. **Errores:** Al presentarse cualquier error crítico o fallo que interrumpa o detenga la ejecución.

---

## Cómo hacerlo (Instrucción de ejecución)
Para enviar la notificación cuando ocurra alguna de las situaciones anteriores, ejecuta directamente el script de comandos por la terminal, **sin escribir código Python adicional** ni scripts intermedios:

```bash
python tools/notify.py "Tu mensaje aquí"
```
