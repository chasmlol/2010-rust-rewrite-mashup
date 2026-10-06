use frame::ScreenEffectsView;
use net::PresentedSnapshot;
use render_material::RuntimeMaterialCatalog;
use render_scene::XModelSurfaceDraw;

pub(super) struct ThermalBodySelection {
    clients: Vec<u32>,
    ordinal: Result<u32, String>,
}

impl ThermalBodySelection {
    pub(super) fn new(
        presented: &PresentedSnapshot,
        effects: &ScreenEffectsView,
        catalog: &RuntimeMaterialCatalog,
    ) -> Self {
        let snapshot = presented.snapshot();
        let clients = snapshot
            .filter(|_| effects.ready && effects.thermal_active)
            .into_iter()
            .flat_map(|snapshot| &snapshot.players)
            .filter(|(_, ps)| ps.perks[0] & playerstate_iw4::PERK_COLDBLOODED == 0)
            .map(|(client, _)| client.0)
            .collect();
        let mut name = material_token(snapshot.map_or("", |snapshot| {
            &snapshot.meta.objectives.thermal_body_material
        }));
        if name.is_empty() {
            name = "thermalbody_default".into();
        }
        let ordinal = catalog
            .ordinal_for_material_name(&name)
            .map(|ordinal| ordinal.get())
            .ok_or(name);
        Self { clients, ordinal }
    }

    pub(super) fn for_draw(&self, draw: &XModelSurfaceDraw) -> Option<&Result<u32, String>> {
        draw.body_client
            .filter(|client| self.clients.contains(client))
            .map(|_| &self.ordinal)
    }
}

fn material_token(text: &str) -> String {
    let mut text = text.split('\0').next().unwrap_or("");
    loop {
        text = text.trim_start_matches(|c: char| c <= ' ');
        if let Some(comment) = text.strip_prefix("//") {
            text = comment.split_once('\n').map_or("", |(_, tail)| tail);
        } else if let Some(comment) = text.strip_prefix("/*") {
            text = comment.split_once("*/").map_or("", |(_, tail)| tail);
        } else {
            break;
        }
    }
    let mut token = Vec::new();
    if let Some(quoted) = text.strip_prefix('"') {
        let mut bytes = quoted.bytes().peekable();
        while let Some(mut byte) = bytes.next() {
            if byte == b'"' {
                break;
            }
            if byte == b'\\' && matches!(bytes.peek(), Some(b'"' | b'\\')) {
                byte = bytes.next().unwrap();
            }
            if token.len() < 1023 {
                token.push(byte);
            }
        }
    } else {
        token.extend(text.bytes().take_while(|byte| *byte > b' ').take(1023));
    }
    String::from_utf8_lossy(&token).into_owned()
}
