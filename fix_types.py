with open('c:/Users/teeag/Xelma-Blockchain/contracts/src/types.rs', 'r', newline='') as f:
    content = f.read()

old = "    /// Timelocked pending critical config change keyed by change kind.\n\n    Per-user outcome record for a specific archived round (round_id, user).\n    Persisted at settlement for user history queries without event replay.\n    UserRoundOutcome(u64, Address),\n    Marker written by migrate_schema_v2_to_v3 to prove the migration ran.\n    MigratedToV3,\n    Timelocked pending critical config change keyed by change kind.\n\n    /// Timelocked pending critical config change keyed by change kind."
new = "    /// Timelocked pending critical config change keyed by change kind."

count = content.count(old)
print(f"Found {count} occurrences")
if count > 0:
    content = content.replace(old, new, 1)
    with open('c:/Users/teeag/Xelma-Blockchain/contracts/src/types.rs', 'w', newline='') as f:
        f.write(content)
    print('Fixed!')
else:
    idx = content.find('PendingConfigChange')
    print(f"PendingConfigChange found at index {idx}")
    print(repr(content[idx-20:idx+20]))
