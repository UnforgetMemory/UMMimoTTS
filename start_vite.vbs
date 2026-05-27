Set WshShell = CreateObject("WScript.Shell")
WshShell.Run "cmd /c cd /d C:\aapp\development\source_code\UMMimoTTS\frontend && npm run dev", 0, False
