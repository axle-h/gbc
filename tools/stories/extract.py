#!/usr/bin/env python3
"""Turn a finished run's hall-of-fame archive into a story digest: plain JSON files a person or a
coding agent can read without knowing the transcript's event vocabulary. Deterministic, no LLM.

    tools/stories/extract.py <archive-dir> <out-dir> [--label haiku-1] [--reconstructed <dir>]

<archive-dir> is `/runs/hall-of-fame/<stamp>-<run-id>/` copied off the PVC. A run older than the
moment tracker has no moments.jsonl or timeline events; --reconstructed points at the dump that
rebuilt them, in the live shapes, and its moments, timeline and snapshots are used instead.

    meta.json             the archive's meta.json, plus the label
    stats.json            totals, badge splits, bosses won/lost, dwell by map, roster, tool use
    moments.jsonl         the live moments, starter-dex artefacts dropped, with turn_i and screen
    moments/              each moment's save, the screen inside it, and a .story.png played forward to
                          the battle, blackout, trainer card or Hall of Fame card
    timeline.jsonl        every change of map, badges, party, owned or seen
    turns.jsonl           one per turn: map, the model's summary and narration, what the game said
    reasoning.jsonl       the model's thinking per turn, by turn_i
    battles.jsonl         each boss battle: outcome, the model's turns in it, the battle text
    plans.jsonl           every distinct version of the model's plan
    battle_scripts.jsonl  every distinct battle script it wrote
    issues.jsonl          every issue it filed
    events.jsonl          notices and compactions
    snapshots*            a reconstructed run's other saves

Play time between timeline samples is interpolated from the wall clock (see Clock).
"""

import argparse
import gzip
import json
import re
import shutil
import subprocess
from collections import Counter, defaultdict
from datetime import datetime, timezone
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
SCREENSHOT = REPO / "target/release/examples/save_screenshot"
ROLL = REPO / "target/release/examples/roll_screens"
# How far past a moment's save to play for a better picture, by kind; see roll_screens.rs.
ROLL_TO = {
    "boss_battle": ["--until", "wants to fight"],
    "blackout": ["--until", "blacked out"],
    "badge": ["--card"],
    "hall_of_fame": ["--until", "Seen:"],
}
# Before the file tools, each thing the model could read was a tool of its own.
OLD_READS = {"read_map": "map.png", "read_guide": "guide/current.md", "read_bag": "bag.md",
             "read_party": "party.md", "read_route": "routes/…", "screenshot": "screenshot"}
HEADLINE = re.compile(r"^(\w+) · (\w+) at \((\d+), (\d+)\)")


def jsonl(path):
    opener = gzip.open if str(path).endswith(".gz") else open
    with opener(path, "rt") as f:
        return [json.loads(line) for line in f if line.strip()]


def write_jsonl(path, rows):
    with open(path, "w") as f:
        for row in rows:
            f.write(json.dumps(row, ensure_ascii=False) + "\n")


def iso(ms):
    return datetime.fromtimestamp(ms / 1000, timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def clock(seconds):
    seconds = int(seconds)
    return f"{seconds // 3600:02}:{seconds // 60 % 60:02}:{seconds % 60:02}"


class Clock:
    """Play time at any wall-clock instant. The game runs while the model thinks, so between two
    timeline samples play time advances with the wall clock, capped at the next sample (a pause on
    quota stops both, and the cap is what keeps that from inflating it)."""

    def __init__(self, timeline):
        self.points = [(t["at"], t["playtime_seconds"]) for t in timeline if "at" in t]

    def at(self, ms):
        lo, hi = 0, len(self.points)
        while lo < hi:
            mid = (lo + hi) // 2
            if self.points[mid][0] <= ms:
                lo = mid + 1
            else:
                hi = mid
        if lo == 0:
            return self.points[0][1] if self.points else 0
        at, play = self.points[lo - 1]
        estimate = play + (ms - at) / 1000
        if lo < len(self.points):
            estimate = min(estimate, self.points[lo][1])
        return int(estimate)


def plan_text(event):
    if "text" in event:
        return event["text"]
    # The old todo-list plan.
    return "\n".join(f"- [{'x' if i.get('done') else ' '}] {i['text']}" for i in event.get("items", []))


def is_starter_artefact(moment):
    """Before the fix, reading the starters' dex pages in Oak's lab registered them as owned."""
    return moment["kind"] == "new_species" and moment["map"] == "OaksLab" and not moment["party"]


def extract(archive, out, label, reconstructed):
    out.mkdir(parents=True, exist_ok=True)
    meta = json.load(open(archive / "meta.json"))
    done = meta["completion"]
    events = jsonl(archive / "transcript.jsonl.gz")

    # Moments and timeline: the live files, or a reconstruction in the same shapes.
    source = reconstructed if reconstructed else archive
    timeline = (
        jsonl(reconstructed / "timeline.jsonl")
        if reconstructed
        else [e for e in events if e["type"] == "timeline"]
    )
    moments = jsonl(source / "moments.jsonl")
    clock_of = Clock(timeline)
    seq_at = {e["seq"]: e["at"] for e in events}

    # Turns: everything from one turn_started to the next belongs to it.
    turns, current = [], None
    reasoning = defaultdict(list)
    cumulative_prev = None
    for e in events:
        t = e["type"]
        if t == "turn_started":
            m = HEADLINE.match(e["headline"])
            current = {
                "i": len(turns) + 1,
                "turn": e["turn"],
                "at": iso(e["at"]),
                "at_ms": e["at"],
                "playtime_seconds": clock_of.at(e["at"]),
                "kind": e["kind"],
                "map": m.group(2) if m else None,
                "x": int(m.group(3)) if m else None,
                "y": int(m.group(4)) if m else None,
                "headline": e["headline"],
                "summary": None,
                "narration": None,
                "said": "",
                "reads": [],
                "rejected": [],
                "plan_edits": 0,
                "issues": [],
                "game": [],
                "tokens": {"prompt": 0, "completion": 0, "cache_read": 0, "cache_write": 0},
                "outcome": None,
            }
            turns.append(current)
            continue
        if current is None:
            continue
        if t == "assistant_reasoning":
            reasoning[current["i"]].append(e["text"])
        elif t == "assistant_delta":
            current["said"] += e["text"]
        elif t == "usage":
            for k, src in (("prompt", "prompt_tokens"), ("completion", "completion_tokens"),
                           ("cache_read", "cache_read_tokens"), ("cache_write", "cache_write_tokens")):
                current["tokens"][k] += e.get(src) or 0
        elif t == "tool_call":
            try:
                args = json.loads(e["arguments"])
            except json.JSONDecodeError:
                args = {}
            if e["kind"] == "read":
                current["reads"].append(args.get("path") or OLD_READS.get(e["name"], e["name"]))
            elif e["kind"] == "rejected":
                current["rejected"].append(args)
            elif e["kind"] == "todo":
                current["plan_edits"] += 1
            elif e["kind"] == "issue":
                current["issues"].append(args)
        elif t == "decision":
            current["summary"] = e.get("summary")
            current["narration"] = e.get("narration")
            # Runs older than per-request usage events carry only the cumulative figure.
            usage = e.get("usage") or {}
            if not any(current["tokens"].values()) and "prompt_tokens" in usage:
                cum = (usage["prompt_tokens"], usage["completion_tokens"])
                prev = cumulative_prev if cumulative_prev and cum[0] >= cumulative_prev[0] else (0, 0)
                current["tokens"]["prompt"] = cum[0] - prev[0]
                current["tokens"]["completion"] = cum[1] - prev[1]
                cumulative_prev = cum
            current["context_tokens"] = usage.get("context_tokens")
            current["outcome"] = "decided"
        elif t == "turn_cancelled":
            current["outcome"] = "cancelled: " + e.get("reason", "")
        elif t == "agent":
            current["game"].append(e["text"])

    turn_by_ms = [(tn["at_ms"], tn["i"]) for tn in turns]

    def turn_at(ms):
        best = None
        for at, i in turn_by_ms:
            if at > ms:
                break
            best = i
        return best

    # Moments: drop the starter-dex artefacts, attach the turn, boss outcomes and screenshots.
    (out / "moments").mkdir(exist_ok=True)
    kept, dropped = [], []
    for m in moments:
        if is_starter_artefact(m):
            dropped.append({"n": m["n"], "title": m["title"], "why": "starter dex page, not a catch"})
            continue
        m = dict(m)
        m["at"] = iso(seq_at[m["seq"]]) if m.get("seq") in seq_at else None
        if m.get("seq") in seq_at:
            m["turn_i"] = turn_at(seq_at[m["seq"]])
        else:
            # The Hall of Fame's moment is recorded after the transcript is archived.
            m["turn_i"] = max((tn["i"] for tn in turns if tn["playtime_seconds"] <= m["playtime_seconds"]), default=None)
        if m.get("state"):
            src = source / m["state"]
            if src.exists():
                shutil.copy2(src, out / m["state"])
                m["screen"] = str(Path(m["state"]).with_suffix(".png"))
        kept.append(m)

    # Boss battles: the battle's own text, from the challenge to the end of the battle.
    battles = []
    index_of_seq = {e["seq"]: i for i, e in enumerate(events)}
    narrations = {tn["i"]: tn for tn in turns}
    for m in kept:
        if m["kind"] != "boss_battle":
            continue
        log, outcome, start = [], "unknown", index_of_seq.get(m["seq"])
        first_turn = m.get("turn_i")
        end_ms = None
        if start is not None:
            for e in events[start:]:
                if e["type"] == "agent" and e["kind"] == "text_box":
                    log.append(e["text"].removeprefix("📖 "))
                    text = e["text"]
                    if re.search(r"\bdefeated [A-Z.]+!", text):
                        outcome = "won"
                    if "blacked out" in text or "out of useable" in text:
                        outcome = "lost"
                if e["type"] == "agent" and e["kind"] == "battle_ended":
                    end_ms = e["at"]
                    break
        # Without a text box (the old dump), a blackout moment in the next minutes says it.
        moves = []
        if first_turn and end_ms:
            for tn in turns[first_turn - 1:]:
                if tn["at_ms"] > end_ms:
                    break
                if tn["narration"] or tn["summary"]:
                    moves.append({"i": tn["i"], "summary": tn["summary"], "narration": tn["narration"]})
        m["outcome"] = m.get("outcome") or outcome
        battles.append({
            "n": m["n"], "title": m["title"], "map": m["map"], "playtime": m["playtime"],
            "outcome": m["outcome"], "party": m["party"],
            "turns_asked": len(moves), "model": moves, "log": log,
        })

    # The model's plan, every version, and its battle scripts.
    plans = [{"version": e.get("version", k), "at": iso(e["at"]), "playtime": clock(clock_of.at(e["at"])),
              "text": plan_text(e)} for k, e in enumerate(x for x in events if x["type"] == "plan")]
    # A clear or a restart republishes the same plan; keep the changes.
    plans = [p for k, p in enumerate(plans) if k == 0 or p["text"] != plans[k - 1]["text"]]
    scripts = [{"at": iso(e["at"]), "playtime": clock(clock_of.at(e["at"])), "source": e["source"]}
               for e in events if e["type"] == "battle_script"]
    scripts = [s for k, s in enumerate(scripts) if k == 0 or s["source"] != scripts[k - 1]["source"]]
    script_state = archive / "battle-script-state.json"

    issues = [{"turn_i": tn["i"], "at": tn["at"], "playtime": clock(tn["playtime_seconds"]), "map": tn["map"],
               "path": a.get("path"), "content": a.get("content")} for tn in turns for a in tn["issues"]]
    housekeeping = [{"at": iso(e["at"]), "playtime": clock(clock_of.at(e["at"])), "type": e["type"],
                     **{k: v for k, v in e.items() if k not in ("seq", "at", "type")}}
                    for e in events if e["type"] in ("notice", "compacted")]

    # Turns out: reasoning split off, since it is most of the bytes.
    write_jsonl(out / "reasoning.jsonl",
                [{"i": i, "text": "".join(parts)} for i, parts in sorted(reasoning.items())])
    for tn in turns:
        tn["playtime"] = clock(tn["playtime_seconds"])
        del tn["at_ms"]
    write_jsonl(out / "turns.jsonl", turns)
    write_jsonl(out / "moments.jsonl", kept)
    write_jsonl(out / "timeline.jsonl", timeline)
    write_jsonl(out / "battles.jsonl", battles)
    write_jsonl(out / "plans.jsonl", plans)
    write_jsonl(out / "battle_scripts.jsonl", scripts)
    write_jsonl(out / "issues.jsonl", issues)
    write_jsonl(out / "events.jsonl", housekeeping)
    if script_state.exists():
        shutil.copy2(script_state, out / "battle-script-state.json")
    if reconstructed and (reconstructed / "snapshots.jsonl").exists():
        shutil.copy2(reconstructed / "snapshots.jsonl", out / "snapshots.jsonl")
        shutil.copytree(reconstructed / "snapshots", out / "snapshots", dirs_exist_ok=True)

    saves = sorted(out.glob("moments/*.pkrd")) + sorted(out.glob("snapshots/*.pkrd"))
    if saves:
        subprocess.run([str(SCREENSHOT), *map(str, saves)], check=True)
    for m in kept:
        if m.get("screen") and m["kind"] in ROLL_TO:
            m["story_screen"] = roll(out, m)
    write_jsonl(out / "moments.jsonl", kept)

    stats = summarise(done, turns, kept, timeline, battles, plans, scripts, issues, housekeeping)
    stats["dropped_moments"] = dropped
    json.dump(stats, open(out / "stats.json", "w"), indent=2, ensure_ascii=False)
    json.dump({"label": label, **meta}, open(out / "meta.json", "w"), indent=2, ensure_ascii=False)
    return stats


def roll(out, moment):
    """The moment's save played forward to the picture ROLL_TO asks for, or None."""
    save = out / moment["state"]
    scratch = save.with_suffix(".roll")
    scratch.mkdir(exist_ok=True)
    try:
        run = subprocess.run([str(ROLL), str(save), str(scratch / "shot"), *ROLL_TO[moment["kind"]]],
                             check=True, capture_output=True, text=True)
        shots = [json.loads(line) for line in run.stdout.splitlines()]
        wanted = ROLL_TO[moment["kind"]]
        hit = [s for s in shots if s["why"] == "card" or (wanted[0] == "--until" and wanted[1] in s["text"])]
        if not hit:
            return None
        target = save.with_suffix(".story.png")
        shutil.copy2(hit[-1]["file"], target)
        return str(target.relative_to(out))
    finally:
        shutil.rmtree(scratch)


def summarise(done, turns, moments, timeline, battles, plans, scripts, issues, housekeeping):
    tokens = Counter()
    for tn in turns:
        tokens.update(tn["tokens"])

    # Splits: what each badge cost, from the one before it.
    splits, prev = [], {"playtime_seconds": 0, "turn_i": 0, "at": None}
    for m in [m for m in moments if m["kind"] in ("badge", "hall_of_fame")]:
        ti = m.get("turn_i") or prev["turn_i"]
        span = turns[prev["turn_i"]:ti]
        splits.append({
            "title": m["title"], "playtime": m["playtime"],
            "play_seconds": m["playtime_seconds"] - prev["playtime_seconds"],
            "turns": len(span),
            "prompt_tokens": sum(t["tokens"]["prompt"] for t in span),
            "blackouts": sum(1 for x in moments if x["kind"] == "blackout"
                             and prev["playtime_seconds"] <= x["playtime_seconds"] < m["playtime_seconds"]),
            "party": m["party"],
        })
        prev = {"playtime_seconds": m["playtime_seconds"], "turn_i": ti}

    # Where the play time went, by map, from the timeline's dwell between samples.
    dwell = Counter()
    for a, b in zip(timeline, timeline[1:]):
        dwell[a["map"]] += b["playtime_seconds"] - a["playtime_seconds"]
    turns_by_map = Counter(tn["map"] for tn in turns)

    # The longest stretches on one map without leaving it: where a run got stuck.
    stuck, run_start = [], None
    for a, b in zip(timeline, timeline[1:] + [None]):
        if run_start is None:
            run_start = a
        if b is None or b["map"] != a["map"]:
            end = b["playtime_seconds"] if b else a["playtime_seconds"]
            stuck.append({"map": a["map"], "from": run_start["playtime"], "seconds": end - run_start["playtime_seconds"]})
            run_start = None
    stuck.sort(key=lambda s: -s["seconds"])

    bosses = defaultdict(lambda: Counter())
    for b in battles:
        bosses[b["title"].removeprefix("challenged ")][b["outcome"]] += 1

    roster = {}
    for t in timeline:
        for p in t.get("party", []):
            key = (p["nickname"], p["species"])
            r = roster.setdefault(key, {"nickname": p["nickname"], "species": p["species"],
                                        "joined": t["playtime"], "max_level": 0})
            r["max_level"] = max(r["max_level"], p["level"])
            r["last_seen"] = t["playtime"]

    reads = Counter()
    for tn in turns:
        for r in tn["reads"]:
            reads[re.sub(r"/[^/]*$", "/…", r) if r and r.count("/") > 1 else r] += 1

    return {
        "model": done["model"],
        "playtime": done["playtime"],
        "wall_hours": round(done["wall_ms"] / 3_600_000, 2),
        "turns": len(turns),
        "turns_decided": sum(1 for t in turns if t["outcome"] == "decided"),
        "tokens": dict(tokens),
        "cache_read_share": round(tokens["cache_read"] / tokens["prompt"], 3) if tokens["cache_read"] else None,
        "meta_tokens": {"prompt": done["prompt_tokens"], "completion": done["completion_tokens"]},
        "moments": dict(Counter(m["kind"] for m in moments)),
        "bosses": {k: dict(v) for k, v in bosses.items()},
        "splits": splits,
        "dwell_by_map_top": dwell.most_common(15),
        "turns_by_map_top": turns_by_map.most_common(15),
        "longest_stays": stuck[:10],
        "roster": list(roster.values()),
        "final_party": done["party"],
        "pokedex": {"owned": done["pokedex_owned"], "seen": done["pokedex_seen"]},
        "money": done["money"],
        "reads": reads.most_common(20),
        "rejected_actions": sum(len(t["rejected"]) for t in turns),
        "plan_versions": len(plans),
        "battle_script_versions": len(scripts),
        "issues": len(issues),
        "compactions": sum(1 for h in housekeeping if h["type"] == "compacted"),
        "turns_cancelled": sum(1 for t in turns if (t["outcome"] or "").startswith("cancelled")),
        "resumes": done["resumes"],
        "watchdog_firings": done["watchdog_firings"],
    }


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("archive", type=Path)
    ap.add_argument("out", type=Path)
    ap.add_argument("--label")
    ap.add_argument("--reconstructed", type=Path)
    a = ap.parse_args()
    s = extract(a.archive, a.out, a.label, a.reconstructed)
    print(f"{a.label}: {s['turns']} turns, {sum(s['moments'].values())} moments, {s['playtime']}")
