# Swiss-file fixture provenance

The Swiss-file fixtures were generated with the pinned `swetest` source and
commands documented in `docs/VALIDATION.md`, replacing `-emos` with `-eswe`,
adding `-edir<temporary-data-directory>`, and selecting objects `-p01D` (Sun,
Moon, Chiron). The sidereal command also uses `-sid1`.

Data repository: `https://github.com/aloistr/swisseph`
Data revision: `cae9ecd4b201544d85e411aced17660932514d43`

| File | SHA-256 |
| --- | --- |
| `sepl_18.se1` | `ca1393ceab3a44fbc895887cf789c68819ae6a1cbc9b22225872dbe4ccd99a66` |
| `semo_18.se1` | `1ca07bd67c24374d77226180c20a4f9996cba013697894810518e7eb582ca4f7` |
| `seas_18.se1` | `a2cd8fc33807c78ca9a700c91c2e042258b12fc4796519e00781440b5ad8b2e2` |

| Output | SHA-256 |
| --- | --- |
| `j2000-greenwich-swiss-tropical.stdout` | `55339cdbe8599945c0d596a6c618c0e56d050b189477c664a404980ca3d35835` |
| `j2000-greenwich-swiss-sidereal-lahiri.stdout` | `55412fe90f0735c26fa6cdbd33b50586cb3202f7f7d09daf765935ae88497f22` |

Only the textual reference output is committed. The `.se1` files and compiled
`swetest` executable remain external.
