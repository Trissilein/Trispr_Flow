' launch-quick.vbs - hidden entry point of the "Trispr Flow" desktop shortcut.
' Runs scripts\launch.mjs: starts the last build at once, rebuilds in the background
' when the sources are newer than that build.
Set sh = CreateObject("WScript.Shell")
dir = Left(WScript.ScriptFullName, InStrRev(WScript.ScriptFullName, "\") - 1) ' ...\scripts
sh.CurrentDirectory = Left(dir, InStrRev(dir, "\") - 1) ' repo root

' 0 = hidden window, False = do not wait.
sh.Run "node scripts\launch.mjs", 0, False
