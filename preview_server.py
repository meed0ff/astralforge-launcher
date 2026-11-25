#!/usr/bin/env python3
"""
Быстрый просмотр веб-интерфейса AstralForge Launcher в браузере.
Запуск: python3 preview_server.py
"""

import http.server
import socketserver
import webbrowser
from pathlib import Path

PORT = 4567
SRC_DIR = Path(__file__).resolve().parent / "launcher" / "src"

class Handler(http.server.SimpleHTTPRequestHandler):
    def __init__(self, *args, **kwargs):
        super().__init__(*args, directory=str(SRC_DIR), **kwargs)

def main():
    print("=" * 60)
    print("🚀 Локальный сервер предварительного просмотра AstralForge UI")
    print(f"📂 Каталог: {SRC_DIR}")
    print(f"🌐 Доступен по адресу: http://localhost:{PORT}")
    print("=" * 60)
    
    # Автоматически открываем браузер
    webbrowser.open(f"http://localhost:{PORT}")
    
    with socketserver.TCPServer(("", PORT), Handler) as httpd:
        try:
            httpd.serve_forever()
        except KeyboardInterrupt:
            print("\nСервер остановлен.")

if __name__ == "__main__":
    main()
