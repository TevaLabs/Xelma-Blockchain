import re
import os

with open('contracts/src/errors.rs', 'r') as f:
    errors_rs = f.read()

variants = []
def replace_variant(m):
    variants.append(m.group(1))
    return f"    {m.group(1)} = {len(variants)},"

errors_rs = re.sub(r'^\s*([A-Za-z0-9_]+)\s*=\s*[0-9]+,', replace_variant, errors_rs, flags=re.MULTILINE)

with open('contracts/src/errors.rs', 'w') as f:
    f.write(errors_rs)

for i, v in enumerate(variants):
    print(f"{i+1}:{v}")
