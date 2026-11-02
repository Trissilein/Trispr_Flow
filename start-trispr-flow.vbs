' start-trispr-flow.vbs - double-click for the dev start of Trispr Flow (hot reload, no window).
' Runs the same "npm run dev" path as the console, so this is always the latest code.
' Quick start without waiting for a build: start-trispr-flow-quick.vbs.
Set sh = CreateObject("WScript.Shell")
sh.CurrentDirectory = Left(WScript.ScriptFullName, InStrRev(WScript.ScriptFullName, "\") - 1)

' Already running? Exit quietly.
If sh.Run("cmd /c tasklist /FI ""IMAGENAME eq trispr-flow.exe"" 2>nul | find /I ""trispr-flow.exe"" >nul", 0, True) = 0 Then WScript.Quit

' 0 = hidden window, False = do not wait.
sh.Run "cmd /c npm run dev", 0, False
