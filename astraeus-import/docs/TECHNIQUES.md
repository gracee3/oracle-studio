# Western chart techniques

`astraeus-techniques` keeps physically cast charts distinct from synthetic
longitude transforms. Progressed and Davison charts contain ordinary derived
charts; harmonics, solar arcs, and midpoint composites produce explicitly
synthetic artifacts.

Implemented method policies are versioned and explicit:

- secondary, tertiary I, tertiary II, and minor progression time keys;
- natal-fixed or recast-at-symbolic-instant angle policy metadata;
- direct Naibod and true-solar-arc directions, applied to all points or angles;
- integer harmonics 2 through 360;
- shortest-arc midpoint composites, with the exact-opposition tie resolved
  zodiac-forward from the first chart;
- Davison midpoint time and spherical geographic midpoint, rejecting
  antipodal locations.

Synthetic houses are never fabricated. Harmonics and solar arcs omit houses.
Composites include cusp midpoints only when `midpoint_angles_and_cusps` is
requested.

Technique artifacts embed the source charts needed to recompute every derived
longitude, symbolic instant, midpoint, and method-defined motion value during
deserialization. Static harmonics and composites have no motion. Continuous
progressions and direct solar arcs express motion per real target day;
stepwise tertiary-I and natal-fixed points have no instantaneous motion.
