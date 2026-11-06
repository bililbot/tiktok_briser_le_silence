@echo off
REM Live Régie — lanceur (fonctionne quel que soit le dossier du projet)
cd /d "%~dp0"

echo ========================================================
echo   LIVE REGIE - lancement de la regie de live
echo ========================================================

REM Fermer les processus fantomes Python si presents
taskkill /F /IM pythonw.exe >nul 2>&1

REM Si une ancienne instance etait orpheline ou bloquee, redemarrer
tasklist /FI "IMAGENAME eq Telecommande_Live_Rust.exe" 2>NUL | find /I /N "Telecommande_Live_Rust.exe">NUL
if "%ERRORLEVEL%"=="0" (
    echo [INFO] Fermeture de l'ancienne instance...
    taskkill /F /IM Telecommande_Live_Rust.exe >nul 2>&1
    timeout /t 1 /nobreak >nul
)

REM Lancer la regie
start "" "%~dp0Telecommande_Live_Rust.exe"

exit /b 0
