pub mod family;
pub use family::{Family, FamilyId, Iw4, Iw5, T5, T6};
pub mod asset_key;
pub mod ident;
pub mod zone_game;

pub use asset_key::{AssetKey, AssetKeyError, AssetKind, AssetNamespace, MaterialKey};
pub use ident::{
    AssetEdge, AssetEdgeCensus, AssetEdgeReason, AssetRef, AssetRefCensus, BoundTarget,
    CatalogIndex, FpvMeshIndex, FpvMeshSpace, FxIndex, FxModelIndex, FxModelSpace, FxSpace,
    IndexSpace, LoadedSoundIndex, LoadedSoundSpace, MapXModelIndex, MapXModelSpace, MaterialIndex,
    MaterialSpace, ProjectileModelIndex, ProjectileModelSpace, SoundAliasIndex, SoundAliasSpace,
    TechniqueSetIndex, TechniqueSetSpace, TracerIndex, TracerSpace, WalkLocalMaterialIndex,
    WorldWeaponIndex, WorldWeaponSpace, XAnimIndex, XAnimSpace, ZoneOwner, bound_zone_names,
};
pub use zone_game::ZoneGame;
