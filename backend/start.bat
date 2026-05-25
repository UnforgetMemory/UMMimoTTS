@echo off
cd /d C:\aapp\development\source_code\UMMimoTTS\backend
set SERVER_PORT=30231
set OUTPUT_DIR=.\data\output
start /B "" "C:\aapp\development\source_code\UMMimoTTS\backend\target\release\um-mimo-tts-server.exe"
echo Server starting on port 30231...
timeout /t 5 /nobreak >nul
