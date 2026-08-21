# Western policy artifacts

`astraeus-western` applies explicit, versioned Western conventions to a neutral
derived chart. It does not alter ephemeris results or calculation identity.

Schema v1 records one rulership policy and one decan policy:

- `traditional_v1` uses the seven classical rulers;
- `modern_v1` retains classical rulers and adds Uranus, Neptune, and Pluto as
  co-rulers of Aquarius, Pisces, and Scorpio;
- `chaldean_faces_v1` follows the repeating Chaldean planetary order; and
- `triplicity_decans_v1` preserves the reviewed element-triplicity rotation
  represented in the legacy requirements.

Annotations include sign element, modality, polarity, sign rulers, decan and
ruler, and domicile/detriment/exaltation/fall. Exact exaltation/fall longitude
and signed distance are recorded for the seven classical planets. V1 does not
score dignities or define terms, sect, reception, accidental dignity, or
interpretation text.

The artifact embeds its derived chart and recomputes every annotation during
deserialization. Unknown policy values, altered annotations, and unsupported
schema versions fail explicitly.
