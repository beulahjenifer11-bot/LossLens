# Step 1: move to the project root (the folder above experiments) and build the release programs
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
cargo build --release --manifest-path protocol\Cargo.toml
$recv = Join-Path $root "protocol\target\release\file_receiver.exe"
$send = Join-Path $root "protocol\target\release\file_sender.exe"
$recvLog = Join-Path $env:TEMP "recv_out.txt"

# Step 2: counters for the final summary
$pass = 0
$fail = 0

# Step 3: every combination of loss percent and seed
foreach ($loss in 0, 1, 5, 10) {
    foreach ($seed in 1, 2, 3) {

        # Step 3a: delete any old output file, then start the receiver in the background
        Remove-Item received.bin -ErrorAction SilentlyContinue
        $p = Start-Process -FilePath $recv -ArgumentList "received.bin $loss $seed" -PassThru -NoNewWindow -RedirectStandardOutput $recvLog
        Start-Sleep -Milliseconds 500

        # Step 3b: run the sender and keep its output
        $out = & $send test.bin

        # Step 3c: wait up to 30 s for the receiver to exit by itself, kill it if it hangs
        $p | Wait-Process -Timeout 30 -ErrorAction SilentlyContinue
        if (-not $p.HasExited) { $p.Kill() }

        # Step 3d: compare the hash of the sent and received files
        $ok = $false
        if (Test-Path received.bin) {
            $ok = ((Get-FileHash test.bin).Hash -eq (Get-FileHash received.bin).Hash)
        }
        if ($ok) { $pass++ } else { $fail++ }

        # Step 3e: print one result line plus the key numbers
        $status = if ($ok) { "PASS" } else { "FAIL" }
        $goodput = ($out | Select-String "Done:").Line
        $retx = ($out | Select-String "Packets sent").Line
        Write-Host "loss=$loss% seed=$seed  $status"
        Write-Host "    $goodput"
        Write-Host "    $retx"
    }
}

# Step 4: summary
Write-Host ""
Write-Host "Summary: $pass passed, $fail failed"