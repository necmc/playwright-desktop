#!/usr/bin/env python3
import os
import sys
import json
import urllib.request

def main():
    if len(sys.argv) < 2:
        print("Uso: python notify.py 'Mensaje'")
        sys.exit(1)

    message = sys.argv[1]
    token = os.getenv("TELEGRAM_BOT_TOKEN")
    chat_id = os.getenv("TELEGRAM_CHAT_ID")

    if not token or not chat_id:
        return  # Si no hay credenciales, sale silenciosamente sin romper nada

    url = f"https://api.telegram.org/bot{token}/sendMessage"
    payload = {
        "chat_id": chat_id,
        "text": message,
        "parse_mode": "Markdown"
    }

    req = urllib.request.Request(
        url,
        data=json.dumps(payload).encode("utf-8"),
        headers={"Content-Type": "application/json"}
    )

    try:
        with urllib.request.urlopen(req, timeout=5):
            pass
    except Exception as e:
        print(f"[Aviso] No se pudo enviar la alerta: {e}")

if __name__ == "__main__":
    main()
