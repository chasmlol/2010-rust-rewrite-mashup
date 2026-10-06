#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LoadedBindingOrigin {
    #[default]
    Unresolved,
    AuthoredName,
    NotLoaded,
    NullAliasExactCompatibility,
    NullAliasSuffixCompatibility,
    MissingConvention,
    AmbiguousConvention {
        matches: usize,
    },
}
