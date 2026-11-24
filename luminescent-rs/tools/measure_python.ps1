<#
Measures the Python Luminescent on Windows the way PERFORMANCE.md asks, so the Rust one has numbers to beat.
The twin of measure_python.sh.

    powershell -ExecutionPolicy Bypass -File luminescent-rs\tools\measure_python.ps1
    powershell -ExecutionPolicy Bypass -File luminescent-rs\tools\measure_python.ps1 -Exe "$env:LOCALAPPDATA\Programs\Luminescent\Luminescent.exe"

-Runs 5          start-up runs to time (one more first, to warm the disk cache, is not counted)
-IdleSecs 60     how long to sit idle before reading CPU and memory
-CopyFrom dir    a data folder to copy and start with (your real one: $env:LOCALAPPDATA\Luminescent), so the
                 numbers include your notes, themes and dashboard. It is copied, never used in place.
-WithHotkeys     register hotkeys like a normal start. Quit your running Luminescent first. Without it the copy
                 runs with LUMINESCENT_NO_HOOK=1 next to yours.

It prints one Markdown table row to paste into rust-rewrite-plan\BASELINE.md.
#>
param(
    [string]$Exe = "",
    [int]$Runs = 5,
    [int]$IdleSecs = 60,
    [string]$CopyFrom = "",
    [switch]$WithHotkeys
)
$ErrorActionPreference = "Stop"

$repo = Resolve-Path (Join-Path $PSScriptRoot "..\..")
if ($Exe) {
    $file = $Exe
    $argList = @()
} else {
    $file = Join-Path $repo ".venv\Scripts\python.exe"
    if (-not (Test-Path $file)) { $file = "python" }
    $argList = @("`"$(Join-Path $repo 'Launcher.py')`"")
}

$work = Join-Path ([System.IO.Path]::GetTempPath()) ("lummeasure-" + [guid]::NewGuid().ToString("N").Substring(0, 8))
$lumHome = Join-Path $work "home"
$env:LUMINESCENT_HOME = $lumHome
if (-not $WithHotkeys) { $env:LUMINESCENT_NO_HOOK = "1" }
$proc = $null

function Fresh-Home {
    if (Test-Path $lumHome) { Remove-Item -Recurse -Force $lumHome }
    New-Item -ItemType Directory -Force $lumHome | Out-Null
    if ($CopyFrom) {
        Copy-Item -Recurse -Force (Join-Path $CopyFrom "*") $lumHome
        Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $lumHome "data\launcher.log*"), (Join-Path $lumHome "data\starting.json")
    }
}

# start -> the "ready:" line in the log (written once every module has started)
function Start-Once {
    Fresh-Home
    $log = Join-Path $lumHome "data\launcher.log"
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $script:proc = Start-Process -FilePath $file -ArgumentList $argList -PassThru -WindowStyle Hidden
    while ($true) {
        if ((Test-Path $log) -and (Select-String -Path $log -Pattern " ready: " -Quiet)) { break }
        if ($script:proc.HasExited) { throw "Luminescent exited before it was ready (exit code $($script:proc.ExitCode))" }
        Start-Sleep -Milliseconds 10
    }
    return $sw.ElapsedMilliseconds
}

function Stop-App {
    if ($script:proc -and -not $script:proc.HasExited) {
        Stop-Process -Id $script:proc.Id -Force
        $script:proc.WaitForExit()
    }
    $script:proc = $null
}

try {
    Write-Host "Timing $Runs starts of: $file $argList"
    $times = @()
    for ($i = 0; $i -le $Runs; $i++) {
        $ms = Start-Once
        Stop-App
        if ($i -gt 0) { $times += $ms; Write-Host "  start ${i}: $ms ms" } else { Write-Host "  warm-up start: $ms ms (not counted)" }
    }
    $sorted = $times | Sort-Object
    $n = $sorted.Count
    $median = if ($n % 2) { $sorted[($n - 1) / 2] } else { [int](($sorted[$n / 2 - 1] + $sorted[$n / 2]) / 2) }
    $best = $sorted[0]

    Write-Host "Sitting idle for $IdleSecs s..."
    Start-Once | Out-Null
    Start-Sleep -Seconds 3                       # let the start-up work finish before counting
    $p = Get-Process -Id $proc.Id
    $cpu0 = $p.TotalProcessorTime.TotalSeconds
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    Start-Sleep -Seconds $IdleSecs
    $p.Refresh()
    $secs = $sw.Elapsed.TotalSeconds
    $cpu = "{0:N2}" -f (($p.TotalProcessorTime.TotalSeconds - $cpu0) / $secs * 100)      # % of one core, like the Linux script
    $rss = "{0:N0}" -f ($p.WorkingSet64 / 1MB)
    $private = "{0:N0}" -f ($p.PrivateMemorySize64 / 1MB)
    $threads = $p.Threads.Count
    $children = @(Get-CimInstance Win32_Process -Filter "ParentProcessId = $($p.Id)").Count
    Stop-App

    Write-Host "Smoke test (builds every page and pop-up once, then quits)..."
    Fresh-Home
    $sw = [System.Diagnostics.Stopwatch]::StartNew()
    $smoke = Start-Process -FilePath $file -ArgumentList ($argList + @("--smoke")) -PassThru -Wait -WindowStyle Hidden
    $smokeMs = $sw.ElapsedMilliseconds
    $smokeOk = if ($smoke.ExitCode -eq 0) { "ok" } else { "FAILED" }

    $cpuName = (Get-CimInstance Win32_Processor | Select-Object -First 1).Name.Trim()
    $os = (Get-CimInstance Win32_OperatingSystem).Caption
    $data = if ($CopyFrom) { "copy of mine" } else { "fresh" }
    $hk = if ($WithHotkeys) { "yes" } else { "no" }
    Write-Host ""
    Write-Host "| Machine | Desktop | Data | Hotkeys | Start to ready (median / best of $Runs) | Idle CPU ($IdleSecs s) | Wake-ups/s | Idle RSS | Threads | Child processes | Smoke test |"
    Write-Host "|---|---|---|---|---|---|---|---|---|---|---|"
    Write-Host "| $cpuName, $([Environment]::ProcessorCount) threads | $os | $data | $hk | $median / $best ms | $cpu % | n/a | $rss MB (private $private MB) | $threads | $children | $smokeMs ms ($smokeOk) |"
}
finally {
    Stop-App
    Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $work
}
