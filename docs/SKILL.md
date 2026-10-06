# Local skill ratings

Account owners retain one Elo rating for each supported IW4 mode. New ratings
start at 1,000 with zero rated events. `war` shares the `tdm` pool.

`updateSkill(first, second, mode, score)` updates two distinct connected
accounts. Score is the first player's result in [0, 1]; the second receives its
complement. The expected result is `1 / (1 + 10^((second-first)/400))` and the
rating transfer is `round(32 * (score-expected))`. Both event counts increase
even when the rounded transfer is zero. The stock kill callback submits a win
in the `tdm` pool. These are IW4L local ratings, independent of platform ranks.

Invalid modes, non-finite scores, missing accounts, self-play and arithmetic
overflow refuse the whole update. Neither account changes on failure.
External script programs and `developer_script` cannot change ratings, using
the same policy as account stats. Bots participate through temporary accounts;
their ratings last only for the match and are excluded from durable saves.

Ratings share the account's revision, owner, signed profile and save receipt.
Listen hosts save authority changes; remote owners receive and save updates
before acknowledging them. Restart retains ratings; a new match carries durable
accounts while clearing client bindings and temporary accounts.

Account format 3 stores ratings after the existing schema-stamped stats buffer.
Formats 1 and 2 upgrade with default ratings; format 2 keeps its signing key.
Stale disk revisions and changed owners still refuse writes. Game-wire version
106 carries ratings in the signed account payload and requires matching peers.
