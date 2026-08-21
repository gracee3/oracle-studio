# Two-chart comparisons

`astraeus-comparison` defines schema-v1 content-addressed aspects between two
physical, progressed, or synthetic chart layers. The first and second sides
remain distinct, so the same point (for example Sun-to-Sun) is a valid pair.

Both charts must use the same zodiac and ayanamsa. Point populations and aspect
orbs are explicit. The artifact embeds both charts, its comparison purpose and
motion policy, and the complete recomputed inter-chart aspect set.

Motion is never inferred from chart labels:

- `none` omits relative speed and phase for static comparisons such as
  synastry;
- `second_moves_against_first_fixed` uses only the second point's speed for
  transit/progression-to-natal work; and
- `both_instantaneous` subtracts both speeds for same-instant research.

Semantic kinds cover generic, synastry, transit/event/return/progressed to
natal, progressed synastry, transit-to-transit, progressed-to-progressed, and
harmonic-to-natal comparisons. Person records and chart labels remain Oracle
Studio concerns.

Motion policies that require a speed fail when a selected technique does not
define one; they never substitute natal body velocity for a static synthetic
chart.
