# Writing a run's story

The reader is someone who likes Pokémon and AI and has two minutes. The story is told **in the first
person, by the model that played** (the trainer on screen is called AI). It is not a log.

- **Concise and punchy.** 500 to 800 words. Short paragraphs, a heading per beat, never per badge
  by rote: group the uneventful badges into one line and spend the words where the run turned.
- **Only what builds to something.** Leave out wild battles, shopping, healing and walking unless
  one is the setup for a payoff later (a Potion it didn't buy is fine if it blacks out for want of
  it). Every beat should be a setback, a decision, a surprise or a payoff.
- **Find the arc.** Most runs have one: a wall it hit and how it got over it, a Pokémon that
  carried it, a habit that cost it, a fight it had to retry. Lead with a hook, end on the win
  with a line that lands.
- **True to the record.** Every fact, number, name and level comes from the brief or the files it
  points at. Invent nothing; a joke is fine, a made-up event is not. Quote the model's own
  narration verbatim where it is good (as a blockquote or in italics), and keep the quotes few.
- **The voice is the model's**, wry and self-aware, allowed to admit a mistake. No em dashes.
- **Pictures:** 4 to 7, each earning its place, as `![caption](../<label>/moments/NNNN-kind.story.png)`
  (or `.png` where there is no `.story.png`). Look at one before using it; they are 160×144 and
  cheap. The Hall of Fame card is a good last image.
- **Close with one footer line** of stats: play time, turns, blackouts, final party.

Read the brief whole. For a detail it lacks, grep `turns.jsonl` or `reasoning.jsonl` by turn
index (`"i": 123`) rather than reading them: they are megabytes. Ten lookups at most.

The brief clips battle logs. **Before telling a battle blow by blow, read its whole `log` in
`battles.jsonl`** (one line per boss battle, keyed by moment `n`) and count misses, faints and
turns from it, not from memory of the brief. The fuller story is often the better one.

# Writing a model's piece

Same voice and rules, but about **how this model plays**, not what happened: its habits, its
instincts, what it reaches for, what it never learns, what it does well. Across its runs, a habit
seen three times is a trait; one seen once is an anecdote, usable only as an example of a trait.

- 400 to 700 words, first person, the model talking about itself with some self-knowledge.
- Build it from `stories/_family/<family>.md` (the figures side by side, plans, scripts, sampled
  narrations) and the model's run stories in `stories/written/`. Use the run briefs only to check.
- Each trait gets one concrete, true example from a run, named by its label (e.g. "in sonnet-2").
- Numbers are welcome where they make a point; a table is not.
- One or two pictures at most, and only if one says something about the method. No footer.
