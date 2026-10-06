# Weapon configuration products

`asset_game::WeaponCatalog` captures source rows. `WeaponBuild` links dependencies
and publishes immutable effective rows, semantic policies and consumer projections.
`weapon_catalog/{capture_merge,catalog_linking,model_linking,publication}` own
those operations; `configuration/{iw4,iw5,t5,t6}` own source selection rules.

`WeaponFamilies` normalizes selections and checks attachments and host limits.
Resolution and UI toggles return a private registry-issued `WeaponHandle` plus
read-only canonical selection. Console/UI use this resolver. Publication iteration
includes the last valid row; unarmed, unknown and unsupported remain distinct.

`WeaponRegistry::bind` rejects foreign revisions before row access. Exact clones
retain identity; republishing changes it. Raw projection getters are crate-private;
consumers use `BoundWeapon`. `PreparedWeapons` admits wire IDs only after checking
the installed snapshot epoch or original event generation. Local revisions never
enter wire IDs/content digests. Queued effects retain their producing generation.

Combat, equipment, penetration, FPV, HUD, world and event projections use one
effective row. Combat binds host rules and location damage before simulation.
The host burst cooldown remains 200 ms across sources. Presentation facts carry
camera, alternate, dual, shield, overlay and event policies without reinterpreting
capture classifications. Loadout labels/archive hints are prepared by asset_game.

`BoundWeapon::preparation` retains source keys and per-component targets/refusals.
T6 models, clips, cues and materials retain T6 source and storage identity.
Missing native dependencies refuse their dependent capabilities; source weapon
rows never inherit another family's row. Publication retains captured family.

The map selects the soldier kit and hands family. Weapon-authored hands are
eligible only for that family. FPV composition validates typed native model
pairs; T6 guns additionally admit an explicit `T6WithIw4Hands` connection for
IW4 soldiers. Catalog owners must match. Unsupported family pairs refuse.

`FpvWeaponTable::bind` checks registry, mesh, clip, material, image and atlas owners
before accepting handles and alternate links. Retained compositions validate model
and hide identity; track mappings/rigs retain actual clips and meshes. Foreign clips
or rig poses refuse before writes. Optional bones/clips and preparation budget remain.
Dropped items and remote kits share demanded `ItemComposition` topology and hide
layout, retaining registry/world owners. Required model/pose failures are cached;
optional attachments and camo-to-base fallback preserve policy. Replacement clears
successes/refusals. Kits retain body/world catalogs; `models()` accepts no catalogs.
Prepared DObj identity controls reuse. Optional heads and separate shield policy remain.
`PlayerAnimationBinding` retains the character kit/rig, paired tree/script and clips.
Multiplayer body-track compatibility validates leaves/tracks before advancement;
tree/script/source family must match the soldier; missing native profiles refuse. Persistent trees reuse only the same binding.
Body/tree/script/clip replacement resets animation and preserves corpse occupation;
unchanged bindings preserve blend, phase and rate.

GPU/media readiness uses separate match, material, FPV and audio milestones.
