#!/usr/bin/env python3
"""Condense a run's digest (extract.py's output) into the brief a story writer reads, one chapter
per badge, small enough to read whole. Deterministic, no LLM.

    tools/stories/brief.py run <digest-dir>                    writes <digest-dir>/brief.md
    tools/stories/brief.py family <out.md> <digest-dir>...    a model's runs side by side, for
                                                               a piece on how that model plays
"""

import json
import re
import sys
from collections import Counter
from pathlib import Path

# Words that tend to mark a narration worth quoting: a setback, a surprise, a change of mind.
NOTABLE = re.compile(
    r"\b(again|stuck|finally|wrong|oops|mistake|lost|failed|fail|can't|cannot|blacked|hmm|wait|actually|"
    r"instead|frustrat|retry|refus|bug|weird|strange|why|loop|giving up|give up|risky|gamble|lucky|"
    r"unlucky|missed|crit|desperate|last chance|no choice|must|careful|lesson)\b",
    re.I,
)


def jsonl(path):
    return [json.loads(line) for line in open(path)] if path.exists() else []


def clip(text, n):
    text = " ".join((text or "").split())
    return text if len(text) <= n else text[: n - 1] + "…"


def clock(seconds):
    seconds = int(seconds)
    return f"{seconds // 3600}:{seconds // 60 % 60:02}"


def party(members):
    return ", ".join(f"{m['nickname']} ({m['species']} L{m['level']})" for m in members) or "none"


def score(turn):
    text = turn.get("narration") or ""
    hits = len(NOTABLE.findall(text))
    return hits * 2 + text.count("!") + (5 if turn["issues"] else 0)


def chapters(d):
    """Turn and play-time bounds of each chapter: up to a badge, and the last up to the Hall of Fame."""
    ends = [m for m in d["moments"] if m["kind"] in ("badge", "hall_of_fame")]
    bounds, start = [], (0, 0)
    for m in ends:
        end = (m.get("turn_i") or start[0], m["playtime_seconds"])
        bounds.append((start, end, m))
        start = end
    return bounds


def load(dir):
    return {
        "dir": dir,
        "meta": json.load(open(dir / "meta.json")),
        "stats": json.load(open(dir / "stats.json")),
        "moments": jsonl(dir / "moments.jsonl"),
        "turns": jsonl(dir / "turns.jsonl"),
        "timeline": jsonl(dir / "timeline.jsonl"),
        "battles": {b["n"]: b for b in jsonl(dir / "battles.jsonl")},
        "plans": jsonl(dir / "plans.jsonl"),
        "scripts": jsonl(dir / "battle_scripts.jsonl"),
        "issues": jsonl(dir / "issues.jsonl"),
        "events": jsonl(dir / "events.jsonl"),
    }


def secs(clock_str):
    h, m, s = map(int, clock_str.split(":"))
    return h * 3600 + m * 60 + s


def run_brief(dir):
    d = load(dir)
    s, done = d["stats"], d["meta"]["completion"]
    turns = d["turns"]
    out = []
    w = out.append
    w(f"# {d['meta']['label']}: {s['model']}, run {done['run_id']}\n")
    w(f"Hall of Fame after {s['playtime']} of play, {s['wall_hours']} h of wall clock, {s['turns']} turns, "
      f"{s['tokens']['prompt'] / 1e6:.0f} M prompt tokens. Trainer name AI. Final party: {party(done['party'])}. "
      f"Pokédex {s['pokedex']['owned']} owned / {s['pokedex']['seen']} seen. Blackouts {s['moments'].get('blackout', 0)}, "
      f"plan versions {s['plan_versions']}, battle-script versions {s['battle_script_versions']}, issues filed "
      f"{s['issues']}, actions refused {s['rejected_actions']}, context compactions {s['compactions']}.\n")
    w("Pictures are paths relative to this file. `.story.png` is the save played forward to the battle "
      "screen, the blackout, the trainer card or the Hall of Fame card; `.png` is the screen the save was taken on.\n")
    w("Lines marked `>` are the model's own one-line narration of a turn, verbatim. `[t123]` is a turn index "
      "into turns.jsonl (field `i`); reasoning.jsonl has the model's thinking for the same index.\n")

    bosses_seen = Counter()
    for k, ((t0, p0), (t1, p1), end) in enumerate(chapters(d), 1):
        span = [t for t in turns if t0 < t["i"] <= t1] if end["kind"] != "hall_of_fame" else [t for t in turns if t["i"] > t0]
        moments = [m for m in d["moments"] if p0 <= m["playtime_seconds"] <= p1 and m is not end]
        w(f"\n## Chapter {k}: {end['title']} at {clock(end['playtime_seconds'])}\n")
        w(f"{len(span)} turns, {clock(p1 - p0)} of play. Party at the end: {party(end['party'])}.\n")
        pic = end.get("story_screen") or end.get("screen")
        if pic:
            w(f"Picture: {pic}\n")

        caught = [m for m in moments if m["kind"] in ("new_species", "evolution")]
        if caught:
            w("New species and evolutions: " + "; ".join(f"{m['title'].replace('registered ', '').replace(' in the Pokédex', '')} "
                                                         f"({clock(m['playtime_seconds'])}, {m['map']})" for m in caught) + "\n")

        for m in moments:
            if m["kind"] == "boss_battle":
                b = d["battles"].get(m["n"], {})
                name = m["title"].removeprefix("challenged ")
                bosses_seen[name] += 1
                attempt = bosses_seen[name]
                outcome = b.get("outcome", "unknown")
                lines = b.get("log", [])
                ending = next((l for l in reversed(lines) if "defeated" in l or "blacked out" in l), lines[-1] if lines else "")
                first_retry = attempt == 1 or outcome == "won"
                w(f"\n- **{clock(m['playtime_seconds'])} {name}, attempt {attempt}: {outcome}** "
                  f"(party {party(m['party'])}) [t{m.get('turn_i')}]")
                w(f"  picture: {m.get('story_screen') or m.get('screen')}" if m.get("screen") else "")
                if first_retry or outcome == "lost":
                    moves = b.get("model", [])
                    picks = moves[:2] + moves[-2:] if len(moves) > 4 else moves
                    for mv in picks:
                        if mv.get("narration"):
                            w(f"  > [t{mv['i']}] {clip(mv['narration'], 220)}")
                    w(f"  game: {clip(ending, 260)}")
            elif m["kind"] == "blackout":
                after = next((t for t in turns if t["i"] > (m.get("turn_i") or 0) and t.get("narration")), None)
                w(f"\n- **{clock(m['playtime_seconds'])} blacked out** on {m['map']} (party {party(m['party'])})"
                  f" picture: {m.get('story_screen') or m.get('screen')}")
                if after:
                    w(f"  > [t{after['i']}] next turn: {clip(after['narration'], 220)}")
            elif m["kind"] == "lull":
                w(f"\n- {clock(m['playtime_seconds'])} {m['title']} on {m['map']}")

        # Where the chapter's play time went: the longest unbroken stays on one map.
        stays, run = [], None
        tl = [t for t in d["timeline"] if p0 <= t["playtime_seconds"] <= p1]
        for a, b in zip(tl, tl[1:] + [None]):
            run = run or a
            if b is None or b["map"] != a["map"]:
                endp = b["playtime_seconds"] if b else a["playtime_seconds"]
                stays.append((endp - run["playtime_seconds"], a["map"], run["playtime_seconds"]))
                run = None
        long = sorted((x for x in stays if x[0] >= 360), reverse=True)[:4]
        if long:
            w("\nLongest stays: " + "; ".join(f"{mp} {x // 60} min from {clock(st)}" for x, mp, st in long))

        notable = sorted((t for t in span if t.get("narration")), key=score, reverse=True)[:8]
        if notable:
            w("\nNotable narrations:")
            for t in sorted(notable, key=lambda t: t["i"]):
                w(f"> [t{t['i']} {t['playtime']} {t['map']}] {clip(t['narration'], 240)}")

        for i in (x for x in d["issues"] if t0 < x["turn_i"] <= max(t1, t0 + 1) or (end["kind"] == "hall_of_fame" and x["turn_i"] > t0)):
            w(f"\nIssue filed [t{i['turn_i']} {i['playtime']} {i['map']}] {i['path']}: {clip(i['content'], 400)}")

        plans = [p for p in d["plans"] if p0 <= secs(p["playtime"]) <= p1]
        if plans:
            w(f"\nPlan: {len(plans)} versions this chapter. The last:\n```\n{clip(plans[-1]['text'], 700)}\n```")
        scripts = [x for x in d["scripts"] if p0 <= secs(x["playtime"]) <= p1]
        if scripts:
            w(f"Battle script: {len(scripts)} new versions this chapter.")

    if d["scripts"]:
        w("\n## The last battle script\n```\n" + clip_lines(d["scripts"][-1]["source"], 40) + "\n```")
    pauses = [e for e in d["events"] if e["type"] == "notice" and re.search(r"quota|paus|resum", e.get("message", ""))]
    if pauses:
        w(f"\nQuota pauses and resumes: {len(pauses)} notices, e.g. \"{clip(pauses[0]['message'], 160)}\".")
    w("\nBiggest time sinks overall: " + "; ".join(f"{m} {s // 60} min" for m, s in s["dwell_by_map_top"][:6]))
    w("Team over the run: " + "; ".join(f"{r['nickname']} the {r['species']} (joined {r['joined']}, max L{r['max_level']})"
                                       for r in s["roster"] if r["max_level"] >= 10))
    (dir / "brief.md").write_text("\n".join(out) + "\n")
    return len("\n".join(out))


def clip_lines(text, n):
    lines = text.splitlines()
    return "\n".join(lines[:n] + ([f"… {len(lines) - n} more lines"] if len(lines) > n else []))


def family_brief(out, dirs):
    """How one model plays, across its runs: the figures that show method rather than events."""
    rows = []
    w = rows.append
    runs = [load(Path(x)) for x in dirs]
    w(f"# How {runs[0]['stats']['model']} plays: {len(runs)} run(s)\n")
    w("Paths are from the repository root: each run's story is stories/written/<label>.md, its brief "
      "stories/<label>/brief.md.\n")
    w("| | " + " | ".join(r["meta"]["label"] for r in runs) + " |")
    w("|---|" + "---|" * len(runs))

    def row(name, f):
        w(f"| {name} | " + " | ".join(str(f(r)) for r in runs) + " |")

    row("play time", lambda r: r["stats"]["playtime"])
    row("turns", lambda r: r["stats"]["turns"])
    row("prompt tokens (M)", lambda r: round(r["stats"]["tokens"]["prompt"] / 1e6))
    row("tokens per turn (k)", lambda r: round(r["stats"]["tokens"]["prompt"] / r["stats"]["turns"] / 1e3, 1))
    row("blackouts", lambda r: r["stats"]["moments"].get("blackout", 0))
    row("boss losses", lambda r: sum(v.get("lost", 0) for v in r["stats"]["bosses"].values()))
    row("species registered", lambda r: r["stats"]["moments"].get("new_species", 0))
    row("party members ever L10+", lambda r: sum(1 for x in r["stats"]["roster"] if x["max_level"] >= 10))
    row("final party levels", lambda r: "/".join(str(m["level"]) for m in r["meta"]["completion"]["party"]))
    row("battle turns share", lambda r: f"{sum(1 for t in r['turns'] if t['kind'] == 'battle') / len(r['turns']):.0%}")
    row("turns chaining 2+ actions", lambda r: f"{sum(1 for t in r['turns'] if ', then' in (t['summary'] or '')) / len(r['turns']):.0%}")
    row("plan versions", lambda r: r["stats"]["plan_versions"])
    row("battle-script versions", lambda r: r["stats"]["battle_script_versions"])
    row("issues filed", lambda r: r["stats"]["issues"])
    row("actions refused", lambda r: r["stats"]["rejected_actions"])
    row("files read", lambda r: sum(n for _, n in r["stats"]["reads"]))
    row("most read", lambda r: ", ".join(f"{p} {n}" for p, n in r["stats"]["reads"][:3]))
    row("avg narration chars", lambda r: round(sum(len(t["narration"] or "") for t in r["turns"]) / len(r["turns"])))

    for r in runs:
        label = r["meta"]["label"]
        w(f"\n## {label}\n")
        w("Split per badge (play time, turns): " + "; ".join(
            f"{sp['title'].replace('earned the ', '')} {clock(sp['play_seconds'])}/{sp['turns']}" for sp in r["stats"]["splits"]))
        if r["plans"]:
            w(f"\nFirst plan:\n```\n{clip(r['plans'][1]['text'] if len(r['plans']) > 1 else r['plans'][0]['text'], 600)}\n```")
            w(f"Last plan:\n```\n{clip(r['plans'][-1]['text'], 600)}\n```")
        if len(r["scripts"]) > 1:
            w(f"First battle script it wrote:\n```\n{clip_lines(r['scripts'][1]['source'], 25)}\n```")
        sample = [t for t in r["turns"] if t.get("narration")]
        step = max(1, len(sample) // 12)
        w("\nNarrations sampled evenly across the run:")
        for t in sample[::step][:12]:
            w(f"> [{t['playtime']} {t['kind']} {t['map']}] {clip(t['narration'], 200)}")
        for i in r["issues"][:4]:
            w(f"\nIssue [{i['playtime']} {i['map']}]: {clip(i['content'], 300)}")
    Path(out).write_text("\n".join(rows) + "\n")


if __name__ == "__main__":
    if sys.argv[1] == "run":
        for x in sys.argv[2:]:
            print(x, run_brief(Path(x)), "chars")
    else:
        family_brief(sys.argv[2], sys.argv[3:])
