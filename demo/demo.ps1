param([int]$Part = 1, [string]$ReceiverIp = "127.0.0.1", [int]$Seed = 1, [int]$FileMB = 5)

# Step 1: stop leftovers, go to the project root, build, stop if the build fails
Get-Process emulator, file_receiver, file_sender -ErrorAction SilentlyContinue | Stop-Process -Force
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
cargo build --release --manifest-path protocol\Cargo.toml
if ($LASTEXITCODE -ne 0) { Write-Host "Build failed, stopping."; exit 1 }
$recv = Join-Path $root "protocol\target\release\file_receiver.exe"
$emu = Join-Path $root "protocol\target\release\emulator.exe"
$send = Join-Path $root "protocol\target\release\file_sender.exe"

# Step 2: the frozen LossLens thresholds, the only place the demo takes them from
$frozen = Join-Path $root "docs\frozen_thresholds.md"
$qf = (Select-String -Path $frozen -Pattern "^- q_frac = (.+)$").Matches[0].Groups[1].Value.Trim()
$sl = (Select-String -Path $frozen -Pattern "^- slope_ms = (.+)$").Matches[0].Groups[1].Value.Trim()

# Step 3: the two runs of this part and the link they run on
if ($Part -eq 1) {
    $runs = @(@{ name = "p1_reno"; cc = "reno" }, @{ name = "p1_losslens"; cc = "losslens" })
    $loss = 1; $rate = 20; $buffer = 80
    $query = "left=p1_reno&llabel=Standard%20sender%20(Reno)&right=p1_losslens&rlabel=LossLens"
} else {
    $runs = @(@{ name = "p2_llcut09"; cc = "llcut09" }, @{ name = "p2_losslens"; cc = "losslens" })
    $loss = 0; $rate = 2; $buffer = 10
    $query = "left=p2_llcut09&llabel=Never%20slows%20down%20(control)&right=p2_losslens&rlabel=LossLens&title=Link%3A%202%20Mbps%2C%20small%20queue%2C%20no%20random%20loss%20(the%20link%20really%20is%20congested)"
}

$trace = "none"
if ($Part -eq 3) {
    $runs = @(@{ name = "p3_reno"; cc = "reno" }, @{ name = "p3_losslens"; cc = "losslens" })
    $loss = 1; $rate = 20; $buffer = 40
    $trace = Join-Path $root "results\trace_wifi.csv"
    $query = "left=p3_reno&llabel=Standard%20sender%20(Reno)&right=p3_losslens&rlabel=LossLens&title=" + [uri]::EscapeDataString("Link: WiFi-like capacity that changes every 0.1 s (replayed from a trace), 40 ms round trip, 1% random loss")
}

# Step 4: the same seeded 5 MB test file as the experiments
$demoRoot = Join-Path $root "results\demo"
New-Item -ItemType Directory $demoRoot -Force | Out-Null
$testFile = Join-Path $demoRoot "test.bin"
$size = $FileMB * 1048576
if ((-not (Test-Path $testFile)) -or ((Get-Item $testFile).Length -ne $size)) {
    $b = New-Object byte[] $size
    (New-Object System.Random 42).NextBytes($b)
    [IO.File]::WriteAllBytes($testFile, $b)
}

# Step 5: tell the presenter where to look
$ip = (Get-NetIPAddress -AddressFamily IPv4 | Where-Object { $_.IPAddress -like "192.168.*" -or $_.IPAddress -like "10.*" -or $_.IPAddress -like "172.*" } | Select-Object -First 1).IPAddress
Write-Host ""
Write-Host "On this laptop open: http://localhost:8000/demo/dashboard.html?$query"
Write-Host "On the phone open:   http://${ip}:8000/demo/dashboard.html?$query"
Write-Host "Receiver address in use: $ReceiverIp (127.0.0.1 means one-laptop rehearsal mode)"
Write-Host ""

foreach ($x in $runs) { Remove-Item (Join-Path $demoRoot $x.name) -Recurse -Force -ErrorAction SilentlyContinue }
# Step 6: run each sender in turn. Same link, same seed, so both see the same drops
foreach ($r in $runs) {
    $dir = Join-Path $demoRoot $r.name
    Remove-Item $dir -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory $dir -Force | Out-Null
    Read-Host "Press Enter to START the run '$($r.name)' (explain what is about to happen first)"

    # Step 6a: one-laptop mode starts its own receiver, two-laptop mode expects the receiver loop on the other laptop
    $pr = $null
    if ($ReceiverIp -eq "127.0.0.1") {
        $pr = Start-Process -FilePath $recv -ArgumentList "$dir\received.bin 0" -WorkingDirectory $dir -PassThru -NoNewWindow -RedirectStandardOutput "$dir\recv_out.txt"
    }

    # Step 6b: the emulator (args: loss percent, delay ms, seed, Mbps, buffer, receiver address), its drop labels go into this run's folder
    $pe = Start-Process -FilePath $emu -ArgumentList "$loss 20 $Seed $rate $buffer ${ReceiverIp}:9001 0 $trace" -WorkingDirectory $dir -PassThru -NoNewWindow -RedirectStandardOutput "$dir\emu_out.txt"
    Start-Sleep -Milliseconds 700

    # Step 6c: the sender, its logs go into this run's folder
    Push-Location $dir
    $out = & $send $testFile "127.0.0.1:9100" $r.cc $qf $sl
    Pop-Location
    $out | ForEach-Object { Write-Host $_ }

    # Step 6d: wait for the emulator (and the local receiver) to stop by themselves
    $pe | Wait-Process -Timeout 40 -ErrorAction SilentlyContinue
    if (-not $pe.HasExited) { $pe.Kill() }
    if ($pr -ne $null) {
        $pr | Wait-Process -Timeout 20 -ErrorAction SilentlyContinue
        if (-not $pr.HasExited) { $pr.Kill() }
        $same = (Test-Path "$dir\received.bin") -and ((Get-FileHash $testFile).Hash -eq (Get-FileHash "$dir\received.bin").Hash)
        Write-Host "File arrived identical: $same"
    }
}
Write-Host ""
Write-Host "Both runs finished. The scoreboard is on the dashboard."


