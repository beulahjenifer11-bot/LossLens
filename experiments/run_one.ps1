param([string]$Cc = "reno", [double]$Loss = 0, [int]$Rate = 20, [int]$Buffer = 80, [int]$Seed = 1, [string]$Name = "run", [int]$FileMB = 5, [string]$Q = "0.2", [string]$Slope = "1.0")

# Step 1: stop leftovers, move to the project root, build, and stop if the build fails
Get-Process emulator, file_receiver, file_sender -ErrorAction SilentlyContinue | Stop-Process -Force
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
cargo build --release --manifest-path protocol\Cargo.toml
if ($LASTEXITCODE -ne 0) { Write-Host "Build failed, stopping."; exit 1 }
$recv = Join-Path $root "protocol\target\release\file_receiver.exe"
$emu = Join-Path $root "protocol\target\release\emulator.exe"
$send = Join-Path $root "protocol\target\release\file_sender.exe"

# Step 2: scratch folder and the same seeded test file the S2 runner uses
$scratch = Join-Path $root "results\scratch"
New-Item -ItemType Directory $scratch -Force | Out-Null
$testFile = Join-Path $scratch "test.bin"
$recvFile = Join-Path $scratch "received.bin"
$recvLog = Join-Path $scratch "recv_out.txt"
$emuLog = Join-Path $scratch "emu_out.txt"
$size = $FileMB * 1048576
if ((-not (Test-Path $testFile)) -or ((Get-Item $testFile).Length -ne $size)) {
    $b = New-Object byte[] $size
    (New-Object System.Random 42).NextBytes($b)
    [IO.File]::WriteAllBytes($testFile, $b)
}
$fileHash = (Get-FileHash $testFile).Hash

# Step 3: start the receiver and the emulator (emulator args: loss, delay ms, seed, Mbps, buffer)
Remove-Item $recvFile -ErrorAction SilentlyContinue
$pr = Start-Process -FilePath $recv -ArgumentList "$recvFile 0" -WorkingDirectory $scratch -PassThru -NoNewWindow -RedirectStandardOutput $recvLog
$pe = Start-Process -FilePath $emu -ArgumentList "$Loss 20 $Seed $Rate $Buffer" -WorkingDirectory $scratch -PassThru -NoNewWindow -RedirectStandardOutput $emuLog
Start-Sleep -Milliseconds 500

# Step 4: run the sender inside the scratch folder and keep its output
Push-Location $scratch
$out = & $send $testFile "127.0.0.1:9100" $Cc $Q $Slope $Q $Slope
Pop-Location

# Step 5: wait for the receiver and emulator to finish by themselves, kill them if they hang
$pr | Wait-Process -Timeout 60 -ErrorAction SilentlyContinue
if (-not $pr.HasExited) { $pr.Kill() }
$pe | Wait-Process -Timeout 30 -ErrorAction SilentlyContinue
if (-not $pe.HasExited) { $pe.Kill() }

# Step 6: did the file arrive identical, and save the three logs under the chosen name
$ok = (Test-Path $recvFile) -and ((Get-FileHash $recvFile).Hash -eq $fileHash)
Copy-Item (Join-Path $scratch "cwnd_log.csv") (Join-Path $root "results\${Name}_cwnd.csv") -Force
Copy-Item (Join-Path $scratch "sender_losses.csv") (Join-Path $root "results\${Name}_losses.csv") -Force
Copy-Item (Join-Path $scratch "emulator_drops.csv") (Join-Path $root "results\${Name}_drops.csv") -Force

# Step 7: show everything
Write-Host ""
$out | ForEach-Object { Write-Host $_ }
Get-Content $emuLog | ForEach-Object { Write-Host $_ }
Write-Host "hash_ok = $ok"
Write-Host "Logs saved as results\${Name}_cwnd.csv, _losses.csv and _drops.csv"