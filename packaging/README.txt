o4fix — DJI O4 Pro gyro noise repair
====================================

Early-2026 DJI O4 Pro air units record noisy motion data during
high-throttle flight, which makes Gyroflow-stabilized footage shudder
and wobble. o4fix rewrites the noisy sections of the video's embedded
motion data in place. The repaired file loads into Gyroflow like a
stock recording.

Quick start
-----------
1. Run o4fix-app.exe
2. Drop your DJI .MP4 files onto the window
3. Click "Start repair"
4. Load the new VIDEO_fixed.MP4 in Gyroflow as usual

Notes
-----
- Your original files are never modified.
- "healthy — nothing to repair" means the recording's motion data is
  fine; just use the original in Gyroflow.
- "Couldn't calibrate motion from this clip" means the clip has no
  calm flight sections to calibrate against; the file is left
  unrepaired rather than risk making it worse.
- o4fix.exe is the command-line version (run: o4fix.exe VIDEO.MP4).
- Keep all files from this zip in one folder (the .dll files are
  required).
- If the app does not start: it needs Microsoft WebView2 (included in
  Windows 11; the app shows a download link on Windows 10 if missing)
  and the Microsoft Visual C++ Runtime
  (https://aka.ms/vs/17/release/vc_redist.x64.exe).

Project: https://github.com/ThaumielSparrow/o4fix
