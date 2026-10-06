# Family identity and composition

`asset_core::FamilyId` identifies IW4, IW5, T5 and T6. `AssetNamespace`,
`ZoneGame` and script `Realm` name this same type at their respective boundaries.
It has no default. Capture builders start without a family; capture requires
explicit selection. Failed map opens return a refusal and publish `MapLoadFailed`.
Empty worlds require an explicit family draw policy.

Assets retain their captured family through native compilation and publication.
T6 weapon rows, common meshes, clips, materials and sounds stay T6. Missing native
components refuse dependent capabilities. Another family's row or material
surface cannot stand in for them. Native T6 sound spatial semantics remain an
unsupported capability until a native cue compiler supplies them.

The map selects soldier kits. A soldier's body and first-person hands belong to
the map's family. Kit arms take precedence over weapon-authored hands; authored
hands are eligible only when they belong to the soldier's family. Missing hands
leave first-person composition unresolved.

`FamilyFpvMesh<F>` is issued by a published catalog after checking family and
owner. `NativeFpvConnection<F>` accepts a gun and hands of the same family.
`T6WithIw4Hands` explicitly accepts T6 guns with IW4 soldier hands. Both reject
mixed catalog owners. Other foreign pairs refuse. Attachments and auxiliary
models must belong to the weapon's family.

Third-person body clips, animation sources, trees and scripts must match the
soldier's family. Missing foreign profiles refuse; IW4 clips are not a fallback.
World policies provide family-specific sky, lighting and shadow inputs. Material
adapters compile those inputs and authored state into shared renderer products.

Simulation still uses the existing host rules. This boundary does not introduce
native rules per game. Weapon rows still contain family-specific extension fields;
registry access and renderer code banks retain their existing interfaces.
