param([int]$Seeds = 10, [int]$SeedStart = 101, [int]$FileMB = 1)

# Step 1: stop leftovers, move to the project root, build once, stop if the build fails
Get-Process emulator, file_receiver, file_sender -ErrorAction SilentlyContinue | Stop-Process -Force
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
cargo build --release --manifest-path protocol\Cargo.toml
if ($LASTEXITCODE -ne 0) { Write-Host "Build failed, stopping."; exit 1 }
$recv = Join-Path $root "protocol\target\release\file_receiver.exe"
$emu = Join-Path $root "protocol\target\release\emulator.exe"
$send = Join-Path $root "protocol\target\release\file_sender.exe"

# Step 2: own scratch folder and a seeded random test file (1 MB keeps each run short)
$scratch = Join-Path $root "results\scratch_tune"
New-Item -ItemType Directory $scratch -Force | Out-Null
$testFile = Join-Path $scratch "test.bin"
$recvFile = Join-Path $scratch "received.bin"
$recvLog = Join-Path $scratch "recv_out.txt"
$emuLog = Join-Path $scratch "emu_out.txt"
$size = $FileMB * 1048576
$b = New-Object byte[] $size
(New-Object System.Random 42).NextBytes($b)
[IO.File]::WriteAllBytes($testFile, $b)
$fileHash = (Get-FileHash $testFile).Hash

# Step 3: results table header, one row per run (tp = overflow drop called CONGESTION, fn = overflow called RANDOM,
# fp = random drop called CONGESTION, tn = random drop called RANDOM, unlabelled = declared lost but not dropped)
$csv = Join-Path $root "results\tuning_grid.csv"
Set-Content $csv "scenario,q_frac,slope_ms,seed,hash_ok,goodput_mbps,tp,fn,fp,tn,unlabelled"

# Step 4: the grid. Both scenarios have 1 percent random loss and 40 ms RTT, they differ in link rate and buffer
$scenarios = @(
    @{ name = "mixed"; rate = 2; buffer = 10 },
    @{ name = "fast"; rate = 20; buffer = 80 }
)
$qList = 0.05, 0.1, 0.2
$slopeList = 0.5, 1, 2, 4
$total = $scenarios.Count * $qList.Count * $slopeList.Count * $Seeds
$n = 0
$clock = [Diagnostics.Stopwatch]::StartNew()

foreach ($seed in $SeedStart..($SeedStart + $Seeds - 1)) {
    foreach ($sc in $scenarios) {
        foreach ($q in $qList) {
            foreach ($slope in $slopeList) {
                $n++

                # Step 4a: start the receiver and the emulator (emulator args: loss percent, delay ms, seed, Mbps, buffer)
                Remove-Item $recvFile -ErrorAction SilentlyContinue
                $pr = Start-Process -FilePath $recv -ArgumentList "$recvFile 0" -WorkingDirectory $scratch -PassThru -NoNewWindow -RedirectStandardOutput $recvLog
                $pe = Start-Process -FilePath $emu -ArgumentList "1 20 $seed $($sc.rate) $($sc.buffer)" -WorkingDirectory $scratch -PassThru -NoNewWindow -RedirectStandardOutput $emuLog
                Start-Sleep -Milliseconds 500

                # Step 4b: run LossLens with this threshold pair
                Push-Location $scratch
                $out = & $send $testFile "127.0.0.1:9100" "losslens" $q $slope
                Pop-Location

                # Step 4c: wait for the receiver and emulator to finish by themselves, kill them if they hang
                $pr | Wait-Process -Timeout 60 -ErrorAction SilentlyContinue
                if (-not $pr.HasExited) { $pr.Kill() }
                $pe | Wait-Process -Timeout 30 -ErrorAction SilentlyContinue
                if (-not $pe.HasExited) { $pe.Kill() }

                # Step 4d: did the file arrive identical, and what goodput did the sender report
                $ok = (Test-Path $recvFile) -and ((Get-FileHash $recvFile).Hash -eq $fileHash)
                $gp = [regex]::Match(($out -join "`n"), "goodput ([\d.]+) Mbps").Groups[1].Value

                # Step 4e: join the sender's declared losses with the emulator's ground truth by packet number
                $truth = @{}
                foreach ($d in (Import-Csv (Join-Path $scratch "emulator_drops.csv"))) { $truth[[int]$d.packet_number] = $d.cause }
                $tp = 0; $fn = 0; $fp = 0; $tn = 0; $unl = 0
                foreach ($l in (Import-Csv (Join-Path $scratch "sender_losses.csv"))) {
                    $cause = $truth[[int]$l.packet_number]
                    if ($cause -eq "QUEUE_OVERFLOW") { if ($l.verdict -eq "CONGESTION") { $tp++ } else { $fn++ } }
                    elseif ($cause -eq "RANDOM") { if ($l.verdict -eq "CONGESTION") { $fp++ } else { $tn++ } }
                    else { $unl++ }
                }

                # Step 4f: one table row and one progress line per run
                Add-Content $csv "$($sc.name),$q,$slope,$seed,$ok,$gp,$tp,$fn,$fp,$tn,$unl"
                $mins = [math]::Round($clock.Elapsed.TotalMinutes, 1)
                Write-Host ("[{0}/{1}] {2} q_frac={3} slope={4} seed={5} hash_ok={6} goodput={7} Mbps ({8} min elapsed)" -f $n, $total, $sc.name, $q, $slope, $seed, $ok, $gp, $mins)
            }
        }
    }
}

# Step 5: done
Write-Host ""
Write-Host "Finished $total runs in $([math]::Round($clock.Elapsed.TotalMinutes, 1)) minutes. Table saved to results\tuning_grid.csv"