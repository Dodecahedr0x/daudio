use proc_macro::TokenStream;

/// Attribute macro that generates nih-plug Plugin/ClapPlugin/Vst3Plugin impls
/// and format exports for a struct implementing `daudio_sdk::DaudioEffect`.
#[proc_macro_attribute]
pub fn daudio_plugin(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // Filled in by a later task. For now, passthrough so the crate compiles.
    item
}
