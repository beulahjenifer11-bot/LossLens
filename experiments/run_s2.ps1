param([int]$Seeds = 3, [int]$FileMB = 5, [int]$SeedStart = 1, [string]$Tag = "runs", [string]$Controllers = "reno,fixed", [string]$LossRates = "0,0.1,0.5,1,2,5")

# Step 1: stop leftovers, move to the project root, build the release programs
Get-Process emulator, file_receiver, file_sender -ErrorAction SilentlyContinue | Stop-Process -Force
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
cargo build --release --manifest-path protocol\Cargo.toml
$recv = Join-Path $root "protocol\target\release\file_receiver.exe"
$emu = Join-Path $root "protocol\target\release\emulator.exe"
$send = Join-Path $root "protocol\target\release\file_sender.exe"

# Step 2: scratch folder for logs, so the experiment never touches your other files
$scratch = Join-Path $root "results\scratch"
New-Item -ItemType Directory $scratch -Force | Out-Null
$testFile = Join-Path $scratch "test.bin"
$recvFile = Join-Path $scratch "received.bin"
$recvLog = Join-Path $scratch "recv_out.txt"
$emuLog = Join-Path $scratch "emu_out.txt"
$csv = Join-Path $root "results\mvp_s2_$Tag.csv"

# Step 3: create the random test file (seeded, so it is the same every time) and remember its hash
$size = $FileMB * 1048576
$b = New-Object byte[] $size
(New-Object System.Random 42).NextBytes($b)
[IO.File]::WriteAllBytes($testFile, $b)
$fileHash = (Get-FileHash $testFile).Hash

# Step 4: results table header, one row will be added per run
Set-Content $csv "controller,loss_pct,seed,hash_ok,seconds,goodput_mbps,srtt_ms,min_rtt_ms,sent,retx,declared,spurious,reductions,max_cwnd_pkts,emu_random,emu_overflow,emu_forwarded,recv_accepted,sack_trunc"

# Step 5: the experiment grid
$lossList = $LossRates.Split(",")
$total = $Controllers.Split(",").Count * $lossList.Count * $Seeds
$n = 0
$clock = [Diagnostics.Stopwatch]::StartNew()

foreach ($cc in $Controllers.Split(",")) {
    foreach ($loss in $lossList) {
        foreach ($seed in $SeedStart..($SeedStart + $Seeds - 1)) {
            $n++

            # Step 5a: start the receiver and the emulator in the background (emulator args: loss, delay ms, seed, Mbps, buffer)
            Remove-Item $recvFile -ErrorAction SilentlyContinue
            $pr = Start-Process -FilePath $recv -ArgumentList "$recvFile 0" -WorkingDirectory $scratch -PassThru -NoNewWindow -RedirectStandardOutput $recvLog
            $pe = Start-Process -FilePath $emu -ArgumentList "$loss 20 $seed 20 80" -WorkingDirectory $scratch -PassThru -NoNewWindow -RedirectStandardOutput $emuLog
            Start-Sleep -Milliseconds 500

            # Step 5b: run the sender inside the scratch folder and keep its output
            Push-Location $scratch
            $out = & $send $testFile "127.0.0.1:9100" $cc
            Pop-Location

            # Step 5c: wait for the receiver and emulator to finish by themselves, kill them if they hang
            $pr | Wait-Process -Timeout 30 -ErrorAction SilentlyContinue
            if (-not $pr.HasExited) { $pr.Kill() }
            $pe | Wait-Process -Timeout 30 -ErrorAction SilentlyContinue
            if (-not $pe.HasExited) { $pe.Kill() }

            # Step 5d: did the file arrive identical?
            $ok = (Test-Path $recvFile) -and ((Get-FileHash $recvFile).Hash -eq $fileHash)

            # Step 5e: pull the numbers out of the sender and emulator text output (empty if the run stalled)
            $text = ($out -join "`n")
            $emuText = [string](Get-Content $emuLog -Raw)
            $m1 = [regex]::Match($text, "in ([\d.]+) s, goodput ([\d.]+) Mbps")
            $m2 = [regex]::Match($text, "srtt ([\d.]+) ms, min RTT ([\d.]+) ms")
            $m3 = [regex]::Match($text, "Packets sent (\d+), retransmissions (\d+) \([\d.]+%\), losses declared (\d+), spurious (\d+)")
            $m4 = [regex]::Match($text, "cwnd reductions (\d+), max cwnd ([\d.]+) packets")
            $m5 = [regex]::Match($emuText, "dropped (\d+) RANDOM\s*\+\s*(\d+) QUEUE_OVERFLOW")
            $recvText = [string](Get-Content $recvLog -Raw); $m6 = [regex]::Match($emuText, "forwarded (\d+) DATA/FIN"); $m7 = [regex]::Match($recvText, "accepted (\d+),"); $m8 = [regex]::Match($recvText, "SACK truncations (\d+)"); $sec = $m1.Groups[1].Value
            $gp = $m1.Groups[2].Value
            $row = "$cc,$loss,$seed,$ok,$sec,$gp,$($m2.Groups[1].Value),$($m2.Groups[2].Value),$($m3.Groups[1].Value),$($m3.Groups[2].Value),$($m3.Groups[3].Value),$($m3.Groups[4].Value),$($m4.Groups[1].Value),$($m4.Groups[2].Value),$($m5.Groups[1].Value),$($m5.Groups[2].Value),$($m6.Groups[1].Value),$($m7.Groups[1].Value),$($m8.Groups[1].Value)"
            Add-Content $csv $row

            # Step 5f: one progress line per run
            $mins = [math]::Round($clock.Elapsed.TotalMinutes, 1)
            Write-Host ("[{0}/{1}] {2} loss={3}% seed={4} hash_ok={5} goodput={6} Mbps ({7} min elapsed)" -f $n, $total, $cc, $loss, $seed, $ok, $gp, $mins)
        }
    }
}

# Step 6: done
Write-Host ""
Write-Host "Finished $total runs in $([math]::Round($clock.Elapsed.TotalMinutes, 1)) minutes. Table saved to results\mvp_s2_runs.csv"