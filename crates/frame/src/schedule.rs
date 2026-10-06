use bevy::prelude::*;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum AuthoritySet {
    Advance,

    Ingress,

    Gather,

    Step,

    Snapshot,

    Fanout,

    Bookkeeping,
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum ClientSet {
    Load,

    Receive,

    Reconcile,

    Input,

    Predict,

    Send,

    Present,

    Ui,

    Effects,

    Diag,
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RenderSet {
    FrontendPrepare,
    Anim,
    Fx,
    FrontendAssemble,
    Gpu,
}

pub fn configure_render_sets(app: &mut App) {
    app.configure_sets(
        Update,
        (
            RenderSet::FrontendPrepare,
            RenderSet::Anim,
            RenderSet::Fx,
            RenderSet::FrontendAssemble,
            RenderSet::Gpu,
        )
            .chain()
            .in_set(ClientSet::Present),
    );
    app.configure_sets(
        PostUpdate,
        (
            RenderSet::FrontendPrepare,
            RenderSet::Anim,
            RenderSet::Fx,
            RenderSet::FrontendAssemble,
            RenderSet::Gpu,
        )
            .chain(),
    );
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ClientEdge(pub u8);

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AuthorityEdge(pub u8);

pub fn client_set_name(set: &ClientSet) -> &'static str {
    match set {
        ClientSet::Load => "Load",
        ClientSet::Receive => "Receive",
        ClientSet::Reconcile => "Reconcile",
        ClientSet::Input => "Input",
        ClientSet::Predict => "Predict",
        ClientSet::Send => "Send",
        ClientSet::Present => "Present",
        ClientSet::Ui => "Ui",
        ClientSet::Effects => "Effects",
        ClientSet::Diag => "Diag",
    }
}

pub fn authority_set_name(set: &AuthoritySet) -> &'static str {
    match set {
        AuthoritySet::Advance => "Advance",
        AuthoritySet::Ingress => "Ingress",
        AuthoritySet::Gather => "Gather",
        AuthoritySet::Step => "Step",
        AuthoritySet::Snapshot => "Snapshot",
        AuthoritySet::Fanout => "Fanout",
        AuthoritySet::Bookkeeping => "Bookkeeping",
    }
}

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct AuthorityBookkeeping;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ClassEquipResolved;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct PresentedPublished;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct LifeFrontPublished;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct FxSoundPublished;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct OwnerEventsPublished;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionSwapApplied;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModelLightingSeated;

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum WorkerCmdSet {
    CellDynBrush = 0,

    CellDynModel = 1,
    CellSceneEnt = 2,
    DpvsEnt = 3,
    BoundEnt = 4,
    SpotShadowEnt = 5,
    FxNonDependent = 6,
    Glass = 7,
    FxRemaining = 8,
    CellStatic = 9,

    AddSceneEnt = 10,
    CellGlass = 11,
    GlassLight = 12,
    GlassVerts = 13,
    MarkVerts = 14,
    DynMarkVerts = 15,
    FxVerts = 16,
    SmodelCache = 17,
    SkinModel = 18,
    FxPhysics = 19,

    Physics = 20,
}

pub fn worker_cmd_name(set: WorkerCmdSet) -> &'static str {
    WORKER_CMD_RETAIL_NAMES[set as usize]
}

pub const WORKER_CMD_RETAIL_NAMES: &[&str] = &[
    "cell_dyn_brush",
    "cell_dyn_model",
    "cell_scene_ent",
    "dpvs_ent",
    "bound_ent",
    "spot_shadow_ent",
    "fx_non_dependent",
    "glass",
    "fx_remaining",
    "cell_static",
    "add_scene_ent",
    "cell_glass",
    "glass_light",
    "glass_verts",
    "mark_verts",
    "dyn_mark_verts",
    "fx_verts",
    "smodelcache",
    "skin_model",
    "fx_physics",
    "physics",
];

pub const WORKER_CMD_TOC: &[WorkerCmdSet] = &[
    WorkerCmdSet::CellDynBrush,
    WorkerCmdSet::CellDynModel,
    WorkerCmdSet::CellSceneEnt,
    WorkerCmdSet::DpvsEnt,
    WorkerCmdSet::BoundEnt,
    WorkerCmdSet::SpotShadowEnt,
    WorkerCmdSet::FxNonDependent,
    WorkerCmdSet::Glass,
    WorkerCmdSet::FxRemaining,
    WorkerCmdSet::CellStatic,
    WorkerCmdSet::AddSceneEnt,
    WorkerCmdSet::CellGlass,
    WorkerCmdSet::GlassLight,
    WorkerCmdSet::GlassVerts,
    WorkerCmdSet::MarkVerts,
    WorkerCmdSet::DynMarkVerts,
    WorkerCmdSet::FxVerts,
    WorkerCmdSet::SmodelCache,
    WorkerCmdSet::SkinModel,
    WorkerCmdSet::FxPhysics,
    WorkerCmdSet::Physics,
];

pub const WORKER_CMD_AFTER: &[(WorkerCmdSet, WorkerCmdSet)] = &[
    (WorkerCmdSet::FxRemaining, WorkerCmdSet::FxNonDependent),
    (WorkerCmdSet::FxRemaining, WorkerCmdSet::Glass),
    (WorkerCmdSet::CellGlass, WorkerCmdSet::Glass),
    (WorkerCmdSet::GlassLight, WorkerCmdSet::CellGlass),
    (WorkerCmdSet::GlassVerts, WorkerCmdSet::CellGlass),
    (WorkerCmdSet::GlassVerts, WorkerCmdSet::GlassLight),
    (WorkerCmdSet::DynMarkVerts, WorkerCmdSet::MarkVerts),
    (WorkerCmdSet::FxPhysics, WorkerCmdSet::Glass),
];

pub const WORKER_CMD_END_FENCE: &[WorkerCmdSet] = &[
    WorkerCmdSet::GlassVerts,
    WorkerCmdSet::MarkVerts,
    WorkerCmdSet::FxVerts,
    WorkerCmdSet::SmodelCache,
    WorkerCmdSet::SkinModel,
];

pub const WORKER_CMD_NOT_RENDER_THREAD: &[WorkerCmdSet] = &[
    WorkerCmdSet::DpvsEnt,
    WorkerCmdSet::BoundEnt,
    WorkerCmdSet::SpotShadowEnt,
    WorkerCmdSet::FxRemaining,
];

pub const AUTHORITY_TOC: &[AuthoritySet] = &[
    AuthoritySet::Advance,
    AuthoritySet::Ingress,
    AuthoritySet::Gather,
    AuthoritySet::Step,
    AuthoritySet::Snapshot,
    AuthoritySet::Fanout,
    AuthoritySet::Bookkeeping,
];

pub const CLIENT_TOC: &[ClientSet] = &[
    ClientSet::Load,
    ClientSet::Receive,
    ClientSet::Reconcile,
    ClientSet::Input,
    ClientSet::Predict,
    ClientSet::Send,
    ClientSet::Present,
    ClientSet::Ui,
    ClientSet::Effects,
    ClientSet::Diag,
];

pub fn configure_authority_sets(app: &mut App) {
    app.configure_sets(
        FixedUpdate,
        (
            AuthoritySet::Advance,
            AuthoritySet::Ingress,
            AuthoritySet::Gather,
            AuthoritySet::Step,
            AuthoritySet::Snapshot,
            AuthoritySet::Fanout,
            AuthoritySet::Bookkeeping,
        )
            .chain(),
    );
    for (index, set) in AUTHORITY_TOC.iter().enumerate() {
        let index = index as u8;
        app.configure_sets(
            FixedUpdate,
            (AuthorityEdge(index), set.clone(), AuthorityEdge(index + 1)).chain(),
        );
    }
    app.configure_sets(
        FixedUpdate,
        AuthorityBookkeeping.in_set(AuthoritySet::Bookkeeping),
    );
}

pub fn configure_client_sets(app: &mut App) {
    app.configure_sets(
        Update,
        (
            ClientSet::Load,
            ClientSet::Receive,
            ClientSet::Reconcile,
            ClientSet::Input,
            ClientSet::Predict,
            ClientSet::Send,
            ClientSet::Present,
            ClientSet::Ui,
            ClientSet::Effects,
            ClientSet::Diag,
        )
            .chain(),
    );
    for (index, set) in CLIENT_TOC.iter().enumerate() {
        let index = index as u8;
        app.configure_sets(
            Update,
            (ClientEdge(index), set.clone(), ClientEdge(index + 1)).chain(),
        );
    }
    app.configure_sets(Update, FxSoundPublished.in_set(ClientSet::Effects));
    app.configure_sets(Update, OwnerEventsPublished.in_set(ClientSet::Predict));
    configure_worker_cmd_sets(app);
}

pub fn configure_worker_cmd_sets(app: &mut App) {
    for set in WORKER_CMD_TOC {
        app.configure_sets(Update, (*set).in_set(ClientSet::Present));
    }
    for &(later, earlier) in WORKER_CMD_AFTER {
        app.configure_sets(Update, later.after(earlier));
    }
    app.configure_sets(
        Update,
        ModelLightingSeated.in_set(WorkerCmdSet::AddSceneEnt),
    );
}
