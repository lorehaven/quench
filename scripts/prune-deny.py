#!/usr/bin/env python3
"""Rewrites the [bans] allowlist in deny.toml to exactly the crates now in the
dependency graph, so the list only ever shrinks as Stage 0 removes crates."""
import json
import os
import subprocess

os.chdir(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
meta = json.loads(subprocess.check_output(["cargo", "metadata", "--format-version", "1", "--all-features"]))
members = set(meta["workspace_members"])
names = sorted({p["name"] for p in meta["packages"] if p["id"] not in members})

text = open("deny.toml").read()
bans = text.index("[bans]")
start = text.index("allow = [\n", bans)
end = text.index("]\n", start) + 2
text = text[:start] + "allow = [\n" + "".join(f'    "{n}",\n' for n in names) + "]\n" + text[end:]
open("deny.toml", "w").write(text)
print(f"{len(names)} crates allowed")
