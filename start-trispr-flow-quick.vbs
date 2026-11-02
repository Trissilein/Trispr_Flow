' start-trispr-flow-quick.vbs - double-click for the quick start of Trispr Flow (no window).
' Runs scripts\launch.mjs: starts the last build at once, rebuilds in the background
' when the sources are newer than that build.
' Works from any clone (C:\GIT, D:\GIT, ...): the repo root is the folder of this file.
Set sh = CreateObject("WScript.Shell")
sh.CurrentDirectory = Left(WScript.ScriptFullName, InStrRev(WScript.ScriptFullName, "\") - 1)

' 0 = hidden window, False = do not wait.
sh.Run "node scripts\launch.mjs", 0, False
