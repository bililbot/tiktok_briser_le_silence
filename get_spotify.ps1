[Console]::OutputEncoding = [System.Text.Encoding]::UTF8
Add-Type -AssemblyName System.Runtime.WindowsRuntime
$asTaskGeneric = ([System.WindowsRuntimeSystemExtensions].GetMethods() | Where-Object {
    $_.Name -eq 'AsTask' -and $_.GetParameters().Count -eq 1 -and $_.GetParameters()[0].ParameterType.Name -eq 'IAsyncOperation`1'
})[0]
Function Await($WinRtTask, $ResultType) {
    $asTask = $asTaskGeneric.MakeGenericMethod($ResultType)
    $netTask = $asTask.Invoke($null, @($WinRtTask))
    $netTask.Wait(-1) | Out-Null
    $netTask.Result
}
[Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager,Windows.Media.Control,ContentType=WindowsRuntime] | Out-Null
$manager = Await ([Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager]::RequestAsync()) ([Windows.Media.Control.GlobalSystemMediaTransportControlsSessionManager])
$sessions = $manager.GetSessions()
$chosen = $null
foreach ($s in $sessions) {
    if ($s.SourceAppUserModelId -match 'Spotify') {
        $chosen = $s
        break
    }
}
if (-not $chosen -and $sessions.Count -gt 0) {
    $chosen = $sessions[0]
}
if ($chosen) {
    $info = Await ($chosen.TryGetMediaPropertiesAsync()) ([Windows.Media.Control.GlobalSystemMediaTransportControlsSessionMediaProperties])
    $pb = $chosen.GetPlaybackInfo()
    $status = if ($pb) { $pb.PlaybackStatus.ToString() } else { 'Playing' }
    $out = @{
        connected = $true
        playing = ($status -eq 'Playing')
        title = [string]$info.Title
        artist = [string]$info.Artist
        album = [string]$info.AlbumTitle
    }
    $out | ConvertTo-Json -Compress
} else {
    '{"connected":false}'
}
