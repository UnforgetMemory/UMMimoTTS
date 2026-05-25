$env:SERVER_PORT = "30231"
$env:OUTPUT_DIR = "./data/output"
Set-Location "C:\aapp\development\source_code\UMMimoTTS\backend"
.\target\release\um-mimo-tts-server.exe
