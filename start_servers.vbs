Set objShell = CreateObject("WScript.Shell")
' Start backend
cmd1 = "cmd /c cd /d C:\aapp\development\source_code\UMMimoTTS && backend\target\release\um-mimo-tts-server.exe --port 30231"
objShell.Run cmd1, 0, False
' Wait a bit for backend to start
WScript.Sleep 8000
' Start vite
cmd2 = "cmd /c cd /d C:\aapp\development\source_code\UMMimoTTS\frontend && npm run dev"
objShell.Run cmd2, 0, False
