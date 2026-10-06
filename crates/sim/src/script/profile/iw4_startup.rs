use crate::script::source::SourceResolver;

/// Install runs the struct initializer itself, before the map's structs exist.
pub struct Iw4Startup {
    pub roots: Vec<String>,
    pub entries: Vec<String>,
}
impl Iw4Startup {
    pub fn new(resolver: &impl SourceResolver, gametype: &str, map: &str) -> Self {
        let gametype = format!("maps/mp/gametypes/{gametype}");
        let map = format!("maps/mp/{map}");
        let callbacks = "maps/mp/gametypes/_callbacksetup";
        let mut roots = vec![
            "codescripts/delete".to_owned(),
            "codescripts/struct".to_owned(),
            callbacks.to_owned(),
            gametype.clone(),
        ];
        let mut entries = vec![format!("{gametype}::main")];
        if resolver.read_bytes(&map).is_ok() {
            entries.push(format!("{map}::main"));
            roots.push(map);
        }
        let t6_equipment = "iw4l_t6/equipment";
        if resolver.read_bytes(t6_equipment).is_ok() {
            roots.push(t6_equipment.to_owned());
            entries.push(format!("{t6_equipment}::main"));
        }
        entries.push(format!("{callbacks}::codecallback_startgametype"));
        Self { roots, entries }
    }
}
