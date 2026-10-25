param([string]$Name = "fast1", [double]$Loss = 1, [int]$Rate = 20, [int]$Buffer = 80, [string]$Controllers = "reno,cubic,fixed,losslens,llcut09", [int]$Seeds = 10, [int]$SeedStart = 1, [int]$FileMB = 5, [int]$Mark = 0, [string]$Trace = "none")

# Step 1: stop leftovers, move to the project root, build, stop if the build fails
Get-Process emulator, file_receiver, file_sender -ErrorAction SilentlyContinue | Stop-Process -Force
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
cargo build --release --manifest-path protocol\Cargo.toml
if ($LASTEXITCODE -ne 0) { Write-Host "Build failed, stopping."; exit 1 }
$recv = Join-Path $root "protocol\target\release\file_receiver.exe"
$emu = Join-Path $root "protocol\target\release\emulator.exe"
$send = Join-Path $root "protocol\target\release\file_sender.exe"

# Step 2: read the frozen thresholds, the only source of LossLens thresholds in final runs
$frozen = Join-Path $root "docs\frozen_thresholds.md"
if (-not (Test-Path $frozen)) { Write-Host "docs\frozen_thresholds.md not found, stopping."; exit 1 }
$qf = (Select-String -Path $frozen -Pattern "^- q_frac = (.+)$").Matches[0].Groups[1].Value.Trim()
$sl = (Select-String -Path $frozen -Pattern "^- slope_ms = (.+)$").Matches[0].Groups[1].Value.Trim()
Write-Host "Frozen thresholds: q_frac $qf, slope_ms $sl"

# Step 3: own scratch folder and the seeded test file
$scratch = Join-Path $root "results\scratch_final"
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

# Step 4: results table header (tp = overflow drop called CONGESTION, fn = overflow called RANDOM, fp = random called CONGESTION, tn = random called RANDOM)
$csv = Join-Path $root "results\final_$Name.csv"
Set-Content $csv "scenario,controller,seed,hash_ok,seconds,goodput_mbps,mean_srtt_ms,p95_srtt_ms,min_rtt_ms,sent,retx,declared,spurious,reductions,max_cwnd_pkts,emu_random,emu_overflow,tp,fn,fp,tn,unlabelled"

# Step 5: the grid, seeds on the outside so a partial run still has balanced data
$ccList = $Controllers.Split(",")
$total = $ccList.Count * $Seeds
$n = 0
$clock = [Diagnostics.Stopwatch]::StartNew()
foreach ($seed in $SeedStart..($SeedStart + $Seeds - 1)) {
    foreach ($cc in $ccList) {
        $n++

        # Step 5a: start the receiver and the emulator (emulator args: loss percent, delay ms, seed, Mbps, buffer)
        Remove-Item $recvFile, $emuLog, (Join-Path $scratch "emulator_drops.csv"), (Join-Path $scratch "sender_losses.csv"), (Join-Path $scratch "cwnd_log.csv") -ErrorAction SilentlyContinue
        $pr = Start-Process -FilePath $recv -ArgumentList "$recvFile 0" -WorkingDirectory $scratch -PassThru -NoNewWindow -RedirectStandardOutput $recvLog
        $pe = Start-Process -FilePath $emu -ArgumentList "$Loss 20 $seed $Rate $Buffer 127.0.0.1:9001 $Mark $Trace" -WorkingDirectory $scratch -PassThru -NoNewWindow -RedirectStandardOutput $emuLog
        Start-Sleep -Milliseconds 500; Write-Host ("emulator running: {0}, emu log exists: {1}, drops file exists: {2}" -f (-not $pe.HasExited), (Test-Path $emuLog), (Test-Path (Join-Path $scratch "emulator_drops.csv")))

        # Step 5b: run the sender with the frozen thresholds (the shadow classifier uses them for every controller)
        Push-Location $scratch
        $out = & $send $testFile "127.0.0.1:9100" $cc $qf $sl
        Pop-Location

        # Step 5c: wait for the receiver and emulator to finish by themselves, kill them if they hang
        $pr | Wait-Process -Timeout 60 -ErrorAction SilentlyContinue
        if (-not $pr.HasExited) { $pr.Kill() }
        $pe | Wait-Process -Timeout 30 -ErrorAction SilentlyContinue
        if (-not $pe.HasExited) { $pe.Kill() }

        # Step 5d: did the file arrive identical, and read the numbers from the sender and emulator output
        $ok = (Test-Path $recvFile) -and ((Get-FileHash $recvFile).Hash -eq $fileHash)
        $text = ($out -join "`n")
        $emuText = [string](Get-Content $emuLog -Raw)
        $m1 = [regex]::Match($text, "in ([\d.]+) s, goodput ([\d.]+) Mbps")
        $m2 = [regex]::Match($text, "srtt ([\d.]+) ms, min RTT ([\d.]+) ms")
        $m3 = [regex]::Match($text, "Packets sent (\d+), retransmissions (\d+) \([\d.]+%\), losses declared (\d+), spurious (\d+)")
        $m4 = [regex]::Match($text, "cwnd reductions (\d+), max cwnd ([\d.]+) packets")
        $m5 = [regex]::Match($emuText, "dropped (\d+) RANDOM\s*\+\s*(\d+) QUEUE_OVERFLOW")

        # Step 5e: mean and 95th percentile of the smoothed RTT over the run, from the cwnd log
        $sr = @(Import-Csv (Join-Path $scratch "cwnd_log.csv") | Where-Object { [double]$_.srtt_ms -gt 0 } | ForEach-Object { [double]$_.srtt_ms } | Sort-Object)
        $meanSrtt = ""
        $p95Srtt = ""
        if ($sr.Count -gt 0) {
            $meanSrtt = [math]::Round(($sr | Measure-Object -Average).Average, 2)
            $p95Srtt = [math]::Round($sr[[math]::Floor(0.95 * ($sr.Count - 1))], 2)
        }

        # Step 5f: join the declared losses with the emulator ground truth by packet number
        $truth = @{}
        foreach ($d in (Import-Csv (Join-Path $scratch "emulator_drops.csv"))) { $truth[[int]$d.packet_number] = $d.cause }
        $tp = 0; $fn = 0; $fp = 0; $tn = 0; $unl = 0
        foreach ($l in (Import-Csv (Join-Path $scratch "sender_losses.csv"))) {
            $cause = $truth[[int]$l.packet_number]
            if ($cause -eq "QUEUE_OVERFLOW") { if ($l.verdict -eq "CONGESTION") { $tp++ } else { $fn++ } }
            elseif ($cause -eq "RANDOM") { if ($l.verdict -eq "CONGESTION") { $fp++ } else { $tn++ } }
            else { $unl++ }
        }

        # Step 5g: one table row and one progress line per run
        $row = "$Name,$cc,$seed,$ok,$($m1.Groups[1].Value),$($m1.Groups[2].Value),$meanSrtt,$p95Srtt,$($m2.Groups[2].Value),$($m3.Groups[1].Value),$($m3.Groups[2].Value),$($m3.Groups[3].Value),$($m3.Groups[4].Value),$($m4.Groups[1].Value),$($m4.Groups[2].Value),$($m5.Groups[1].Value),$($m5.Groups[2].Value),$tp,$fn,$fp,$tn,$unl"
        Add-Content $csv $row
        $mins = [math]::Round($clock.Elapsed.TotalMinutes, 1)
        Write-Host ("[{0}/{1}] {2} seed={3} hash_ok={4} goodput={5} Mbps ({6} min elapsed)" -f $n, $total, $cc, $seed, $ok, $m1.Groups[2].Value, $mins)
    }
}

# Step 6: done
Write-Host ""
Write-Host "Finished $total runs in $([math]::Round($clock.Elapsed.TotalMinutes, 1)) minutes. Table saved to results\final_$Name.csv"

