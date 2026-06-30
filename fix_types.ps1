$c = Get-Content 'c:/Users/teeag/Xelma-Blockchain/contracts/src/types.rs' -Raw
$old = '    MigratedToV3,`r`n    /// Timelocked pending critical config change keyed by change kind.`r`n`r`n    Per-user outcome record for a specific archived round (round_id, user).`r`n    Persisted at settlement for user history queries without event replay.`r`n    UserRoundOutcome(u64, Address),`r`n    Marker written by migrate_schema_v2_to_v3 to prove the migration ran.`r`n    MigratedToV3,`r`n    Timelocked pending critical config change keyed by change kind.`r`n`r`n    /// Timelocked pending critical config change keyed by change kind.'
$new = '    MigratedToV3,`r`n    /// Timelocked pending critical config change keyed by change kind.'
if ($c.Contains($old)) {
    $c = $c.Replace($old, $new)
    Set-Content 'c:/Users/teeag/Xelma-Blockchain/contracts/src/types.rs' $c -NoNewline
    Write-Host 'SUCCESS: File fixed'
} else {
    Write-Host 'FAILED: Pattern not found'
}
