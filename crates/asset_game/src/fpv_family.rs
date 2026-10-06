use crate::FpvMeshIndex;
use asset_core::{Family, FamilyId, Iw4, Iw5, T5, T6};
use asset_model::{FamilyFpvMesh, FpvMeshCatalog};

#[derive(Clone, Copy, Debug)]
pub struct NativeFpvConnection<F: Family> {
    gun: FamilyFpvMesh<F>,
    hands: FamilyFpvMesh<F>,
}
impl<F: Family> NativeFpvConnection<F> {
    pub fn connect(gun: FamilyFpvMesh<F>, hands: FamilyFpvMesh<F>) -> Option<Self> {
        (gun.owner() == hands.owner()).then_some(Self { gun, hands })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct T6WithIw4Hands {
    gun: FamilyFpvMesh<T6>,
    hands: FamilyFpvMesh<Iw4>,
}
impl T6WithIw4Hands {
    pub fn connect(gun: FamilyFpvMesh<T6>, hands: FamilyFpvMesh<Iw4>) -> Option<Self> {
        (gun.owner() == hands.owner()).then_some(Self { gun, hands })
    }
}

#[derive(Clone, Copy, Debug)]
pub enum FpvFamilyConnection {
    Iw4(NativeFpvConnection<Iw4>),
    Iw5(NativeFpvConnection<Iw5>),
    T5(NativeFpvConnection<T5>),
    T6(NativeFpvConnection<T6>),
    T6WithIw4Hands(T6WithIw4Hands),
}
impl FpvFamilyConnection {
    pub(crate) fn bind(
        catalog: &FpvMeshCatalog,
        gun: FpvMeshIndex,
        hands: FpvMeshIndex,
    ) -> Option<Self> {
        match catalog.get_at(gun.order())?.namespace {
            FamilyId::Iw4 => {
                NativeFpvConnection::connect(catalog.family_mesh(gun)?, catalog.family_mesh(hands)?)
                    .map(Self::Iw4)
            }
            FamilyId::Iw5 => {
                NativeFpvConnection::connect(catalog.family_mesh(gun)?, catalog.family_mesh(hands)?)
                    .map(Self::Iw5)
            }
            FamilyId::T5 => {
                NativeFpvConnection::connect(catalog.family_mesh(gun)?, catalog.family_mesh(hands)?)
                    .map(Self::T5)
            }
            FamilyId::T6 => {
                let gun = catalog.family_mesh(gun)?;
                if let Some(hands) = catalog.family_mesh::<T6>(hands) {
                    NativeFpvConnection::connect(gun, hands).map(Self::T6)
                } else {
                    T6WithIw4Hands::connect(gun, catalog.family_mesh::<Iw4>(hands)?)
                        .map(Self::T6WithIw4Hands)
                }
            }
        }
    }
    pub fn family(self) -> FamilyId {
        match self {
            Self::Iw4(_) => FamilyId::Iw4,
            Self::Iw5(_) => FamilyId::Iw5,
            Self::T5(_) => FamilyId::T5,
            Self::T6(_) | Self::T6WithIw4Hands(_) => FamilyId::T6,
        }
    }
    pub fn gun(self) -> FpvMeshIndex {
        match self {
            Self::Iw4(c) => c.gun.index(),
            Self::Iw5(c) => c.gun.index(),
            Self::T5(c) => c.gun.index(),
            Self::T6(c) => c.gun.index(),
            Self::T6WithIw4Hands(c) => c.gun.index(),
        }
    }
    pub fn hands(self) -> FpvMeshIndex {
        match self {
            Self::Iw4(c) => c.hands.index(),
            Self::Iw5(c) => c.hands.index(),
            Self::T5(c) => c.hands.index(),
            Self::T6(c) => c.hands.index(),
            Self::T6WithIw4Hands(c) => c.hands.index(),
        }
    }
}
