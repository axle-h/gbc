#!/usr/bin/env bash
# Copy every finished run's hall-of-fame archive off the deployment's PVC into stories/_raw/, then
# extract a digest of each into stories/<label>/. `--no-fetch` reuses what is already copied.
set -euo pipefail
cd "$(dirname "$0")/../.."

raw=stories/_raw
mkdir -p "$raw"
if [[ "${1:-}" != --no-fetch ]]; then
  pod=$(kubectl -n gb get pod -l app=gb -o jsonpath='{.items[0].metadata.name}')
  kubectl -n gb cp "$pod:/runs/hall-of-fame/ledger.jsonl" "$raw/ledger.jsonl" 2>/dev/null
  for archive in $(jq -r .archive "$raw/ledger.jsonl"); do
    [[ -d "$raw/$archive" ]] || kubectl -n gb cp "$pod:/runs/hall-of-fame/$archive" "$raw/$archive" 2>/dev/null
  done
fi

cargo build --release -q -p pokered --example save_screenshot
cargo build --release -q -p poke-agent --example roll_screens

# A label is the model's family and its ordinal among that model's runs, oldest first.
declare -A seen
while read -r archive model; do
  family=$(sed -E 's/^claude-([a-z]+)-.*/\1/; s#^openrouter/free$#free#' <<<"$model")
  seen[$family]=$(( ${seen[$family]:-0} + 1 ))
  label="$family-${seen[$family]}"
  run=${archive#*-*-}
  extra=()
  [[ -f "$raw/$archive/moments.jsonl" ]] || extra=(--reconstructed "stories/_reconstructed/$run")
  rm -rf "stories/$label"
  python3 tools/stories/extract.py "$raw/$archive" "stories/$label" --label "$label" "${extra[@]}"
done < <(jq -r '"\(.archive) \(.model)"' "$raw/ledger.jsonl")

python3 - <<'PY'
import json, pathlib
rows = []
for stats in sorted(pathlib.Path("stories").glob("*/stats.json")):
    s, m = json.load(open(stats)), json.load(open(stats.parent / "meta.json"))
    rows.append({"label": m["label"], "run_id": m["completion"]["run_id"], "model": s["model"],
                 "started": m["completion"]["started_at"], "playtime": s["playtime"], "turns": s["turns"],
                 "prompt_tokens": s["tokens"]["prompt"], "moments": s["moments"], "final_party": s["final_party"]})
rows.sort(key=lambda r: r["started"])
json.dump(rows, open("stories/index.json", "w"), indent=2, ensure_ascii=False)
PY

python3 tools/stories/brief.py run stories/*-[0-9]
mkdir -p stories/_family
for family in $(ls -d stories/*-[0-9] | sed -E 's#stories/(.*)-[0-9]+#\1#' | sort -u); do
  python3 tools/stories/brief.py family "stories/_family/$family.md" stories/"$family"-[0-9]
done
