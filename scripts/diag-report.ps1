# Prints one row per transfer summary from a Relayless transfers.jsonl file.
# Usage:
#   powershell -File scripts\diag-report.ps1 [path\to\transfers.jsonl] [-Last 20] [-Json]
# Default path: %APPDATA%\app.relayless.desktop\diagnostics\transfers.jsonl
param(
    [string]$Path = "$env:APPDATA\app.relayless.desktop\diagnostics\transfers.jsonl",
    [int]$Last = 20,
    [switch]$Json
)

if (-not (Test-Path $Path)) {
    Write-Error "No summaries at $Path. Is diagnostics on (RELAYLESS_DIAG not 0) and has a transfer finished?"
    exit 1
}

function Sec($ms) { if ($null -eq $ms) { "" } else { "{0:N1}" -f ($ms / 1000) } }

$rows = Get-Content $Path | Where-Object { $_.Trim() } | ForEach-Object { $_ | ConvertFrom-Json } |
    Where-Object { $_.kind -eq "transfer_summary" } | Select-Object -Last $Last

if ($Json) { $rows | ConvertTo-Json -Depth 12; exit 0 }

$rows | ForEach-Object {
    $t = $_.transfer
    $tm = $_.timings_ms
    $sel = @($_.connection.at_end.paths | Where-Object { $_.selected })[0]
    $limit = if ($_.role -eq "receiver") { $_.configuration.download_limit } else { $_.configuration.upload_limit }
    [pscustomobject]@{
        role       = $_.role
        id         = $_.transfer_id.Substring(0, [Math]::Min(10, $_.transfer_id.Length))
        file_key   = $_.file_key
        peer       = $_.peer
        outcome    = $t.outcome
        route      = "$($_.route.initial)->$($_.route.final)"
        relay_MiB  = [Math]::Round($_.route.bytes_by_route.relay / 1MB, 1)
        MiB        = [Math]::Round($t.bytes_transferred / 1MB, 1)
        elapsed_s  = Sec $t.elapsed_ms
        avg_MiB_s  = $t.rates.active_average_mib_per_sec
        streams    = "$($t.average_active_streams_in_active_window)/$($t.active_streams.peak)"
        limit      = if ($limit.unlimited) { "none" } else { "$($limit.configured_value) KiB/s" }
        limiter_s  = Sec $tm.rate_limiter_wait.total_ms
        read_s     = Sec $tm.quic_read_wait.total_ms
        qwrite_s   = Sec $tm.quic_write_blocked.total_ms
        disk_s     = Sec ($(if ($_.role -eq "receiver") { $tm.disk_write.total_ms } else { $tm.disk_read.total_ms }))
        hash_s     = Sec ($(if ($_.role -eq "receiver") { $tm.hash_verify.total_ms } else { $tm.hash.total_ms }))
        rtt_ms     = $sel.rtt_ms
        cwnd       = $sel.cwnd_bytes
        pacing_B_s = $sel.pacing_rate_bytes_per_sec
        lost       = $sel.lost_packets
        errors     = (($_.errors.PSObject.Properties | ForEach-Object { "$($_.Name)=$($_.Value)" }) -join " ")
    }
} | Format-Table -Property * -AutoSize | Out-String -Width 4096
