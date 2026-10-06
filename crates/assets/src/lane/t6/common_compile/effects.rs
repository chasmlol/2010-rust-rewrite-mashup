use crate::lane::CommonDependencyRefusal;
use asset_material::MaterialCatalog;

pub(super) fn bind_t6_fx(
    content: &mut super::super::T6Content,
    materials: &mut MaterialCatalog,
    catalog: &mut asset_game::FxCatalog,
    refusals: &mut Vec<CommonDependencyRefusal>,
) -> String {
    let bound = super::models::bind_native_materials(
        &content.path,
        &content.fx_materials,
        &content.techsets,
        materials,
        refusals,
    );
    let count = content.fx.len();
    for fx in &content.fx {
        catalog.capture_t6(fx, asset_core::FamilyId::T6);
    }
    format!(
        "t6 native effects: {count} effects, {} materials",
        bound.len()
    )
}
