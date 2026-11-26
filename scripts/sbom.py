#!/usr/bin/env python3
"""Generate a CycloneDX 1.5 component/dependency inventory from Cargo metadata.

Includes the resolved workspace graph, not an attestation of binary reachability.
No third-party Python packages or network requests beyond Cargo's metadata lookup.
"""
import json
from pathlib import Path
import subprocess
import sys

metadata = json.loads(subprocess.check_output([
    "cargo", "metadata", "--locked", "--all-features", "--format-version", "1"
], text=True))
components = []
refs = {}
for package in metadata["packages"]:
    ref = "pkg:cargo/" + package["name"] + "@" + package["version"]
    refs[package["id"]] = ref
    item = {"type": "library", "bom-ref": ref, "name": package["name"],
            "version": package["version"], "purl": ref}
    if package.get("license"):
        item["licenses"] = [{"expression": package["license"]}]
    if package.get("repository"):
        item["externalReferences"] = [{"type": "vcs", "url": package["repository"]}]
    components.append(item)
dependencies = [
    {"ref": refs[node["id"]], "dependsOn": sorted({refs[d] for d in node["dependencies"]})}
    for node in metadata["resolve"]["nodes"]
]
output = {"bomFormat": "CycloneDX", "specVersion": "1.5", "version": 1,
          "components": sorted(components, key=lambda c: c["bom-ref"]),
          "dependencies": sorted(dependencies, key=lambda d: d["ref"])}
path = Path(sys.argv[1] if len(sys.argv) > 1 else "sbom.cdx.json")
path.write_text(json.dumps(output, indent=2) + "\n")
print(f"Wrote {len(components)} components to {path}")
