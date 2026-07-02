use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{parse_macro_input, Ident, ItemStruct, LitByteStr, LitStr, Token};

/// Parsed contents of the `#[daudio_plugin(...)]` attribute.
struct PluginArgs {
    name: LitStr,
    vendor: LitStr,
    url: Option<LitStr>,
    email: Option<LitStr>,
    clap_id: LitStr,
    clap_description: Option<LitStr>,
    vst3_id: LitStr,
    clap_features: Vec<Ident>,
    vst3_categories: Vec<Ident>,
}

/// A single `key = value` entry in the attribute list. `value` is either a
/// string literal or a bracketed list of idents.
enum ArgValue {
    Str(LitStr),
    Idents(Vec<Ident>),
}

struct Arg {
    key: Ident,
    value: ArgValue,
}

impl Parse for Arg {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let key: Ident = input.parse()?;
        input.parse::<Token![=]>()?;
        let value = if input.peek(syn::token::Bracket) {
            let content;
            syn::bracketed!(content in input);
            let idents: Punctuated<Ident, Token![,]> =
                content.parse_terminated(Ident::parse, Token![,])?;
            ArgValue::Idents(idents.into_iter().collect())
        } else {
            ArgValue::Str(input.parse()?)
        };
        Ok(Arg { key, value })
    }
}

impl Parse for PluginArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let args: Punctuated<Arg, Token![,]> = Punctuated::parse_terminated(input)?;

        let mut name = None;
        let mut vendor = None;
        let mut url = None;
        let mut email = None;
        let mut clap_id = None;
        let mut clap_description = None;
        let mut vst3_id = None;
        let mut clap_features = None;
        let mut vst3_categories = None;

        for arg in args {
            let key = arg.key;
            let key_str = key.to_string();

            // Reject duplicate keys instead of silently last-wins.
            macro_rules! assign {
                ($slot:ident, $val:expr) => {{
                    if $slot.is_some() {
                        return Err(syn::Error::new(
                            key.span(),
                            format!("duplicate `daudio_plugin` key `{key_str}`"),
                        ));
                    }
                    $slot = Some($val);
                }};
            }

            match (key_str.as_str(), arg.value) {
                ("name", ArgValue::Str(s)) => assign!(name, s),
                ("vendor", ArgValue::Str(s)) => assign!(vendor, s),
                ("url", ArgValue::Str(s)) => assign!(url, s),
                ("email", ArgValue::Str(s)) => assign!(email, s),
                ("clap_id", ArgValue::Str(s)) => assign!(clap_id, s),
                ("clap_description", ArgValue::Str(s)) => assign!(clap_description, s),
                ("vst3_id", ArgValue::Str(s)) => assign!(vst3_id, s),
                ("clap_features", ArgValue::Idents(v)) => assign!(clap_features, v),
                ("vst3_categories", ArgValue::Idents(v)) => assign!(vst3_categories, v),
                (other, _) => {
                    return Err(syn::Error::new(
                        key.span(),
                        format!("unknown or mistyped `daudio_plugin` key `{other}`"),
                    ));
                }
            }
        }

        let require = |opt: Option<LitStr>, field: &str| {
            opt.ok_or_else(|| {
                syn::Error::new(
                    proc_macro2::Span::call_site(),
                    format!("`daudio_plugin` requires a `{field}` key"),
                )
            })
        };

        Ok(PluginArgs {
            name: require(name, "name")?,
            vendor: require(vendor, "vendor")?,
            url,
            email,
            clap_id: require(clap_id, "clap_id")?,
            clap_description,
            vst3_id: require(vst3_id, "vst3_id")?,
            clap_features: clap_features.ok_or_else(|| {
                syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "`daudio_plugin` requires a `clap_features` key",
                )
            })?,
            vst3_categories: vst3_categories.ok_or_else(|| {
                syn::Error::new(
                    proc_macro2::Span::call_site(),
                    "`daudio_plugin` requires a `vst3_categories` key",
                )
            })?,
        })
    }
}

/// Attribute macro that generates nih-plug Plugin/ClapPlugin/Vst3Plugin impls
/// and format exports for a struct implementing `daudio_sdk::DaudioEffect`.
#[proc_macro_attribute]
pub fn daudio_plugin(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = parse_macro_input!(attr as PluginArgs);
    let item_struct = parse_macro_input!(item as ItemStruct);
    let ident = &item_struct.ident;

    // Validate the VST3 class id is exactly 16 ASCII bytes.
    let vst3_id_value = args.vst3_id.value();
    if vst3_id_value.len() != 16 || !vst3_id_value.is_ascii() {
        return syn::Error::new(
            args.vst3_id.span(),
            "`vst3_id` must be exactly 16 ASCII bytes",
        )
        .to_compile_error()
        .into();
    }

    let PluginArgs {
        name,
        vendor,
        url,
        email,
        clap_id,
        clap_description,
        vst3_id,
        clap_features,
        vst3_categories,
    } = args;

    let url_ts: TokenStream2 = match &url {
        Some(u) => quote! { #u },
        None => quote! { "" },
    };
    let email_ts: TokenStream2 = match &email {
        Some(e) => quote! { #e },
        None => quote! { "" },
    };
    let clap_description_ts: TokenStream2 = match &clap_description {
        Some(d) => quote! { ::std::option::Option::Some(#d) },
        None => quote! { ::std::option::Option::None },
    };

    let clap_features_ts = clap_features
        .iter()
        .map(|f| quote! { ::daudio_sdk::nih_plug::prelude::ClapFeature::#f });
    let vst3_categories_ts = vst3_categories
        .iter()
        .map(|c| quote! { ::daudio_sdk::nih_plug::prelude::Vst3SubCategory::#c });

    let vst3_id_bytes = LitByteStr::new(vst3_id_value.as_bytes(), vst3_id.span());

    let expanded = quote! {
        #item_struct

        impl ::daudio_sdk::nih_plug::prelude::Plugin for #ident {
            const NAME: &'static str = #name;
            const VENDOR: &'static str = #vendor;
            const URL: &'static str = #url_ts;
            const EMAIL: &'static str = #email_ts;
            const VERSION: &'static str = env!("CARGO_PKG_VERSION");

            const AUDIO_IO_LAYOUTS: &'static [::daudio_sdk::nih_plug::prelude::AudioIOLayout] =
                &[::daudio_sdk::nih_plug::prelude::AudioIOLayout {
                    main_input_channels: ::std::num::NonZeroU32::new(2),
                    main_output_channels: ::std::num::NonZeroU32::new(2),
                    ..::daudio_sdk::nih_plug::prelude::AudioIOLayout::const_default()
                }];

            const SAMPLE_ACCURATE_AUTOMATION: bool = true;

            type SysExMessage = ();
            type BackgroundTask = ();

            fn params(&self) -> ::std::sync::Arc<dyn ::daudio_sdk::nih_plug::prelude::Params> {
                self.params.clone()
            }

            fn initialize(
                &mut self,
                _layout: &::daudio_sdk::nih_plug::prelude::AudioIOLayout,
                buffer_config: &::daudio_sdk::nih_plug::prelude::BufferConfig,
                _context: &mut impl ::daudio_sdk::nih_plug::prelude::InitContext<Self>,
            ) -> bool {
                <Self as ::daudio_sdk::DaudioEffect>::activate(self, buffer_config.sample_rate);
                true
            }

            fn reset(&mut self) {
                <Self as ::daudio_sdk::DaudioEffect>::reset(self);
            }

            fn process(
                &mut self,
                buffer: &mut ::daudio_sdk::nih_plug::prelude::Buffer,
                _aux: &mut ::daudio_sdk::nih_plug::prelude::AuxiliaryBuffers,
                _context: &mut impl ::daudio_sdk::nih_plug::prelude::ProcessContext<Self>,
            ) -> ::daudio_sdk::nih_plug::prelude::ProcessStatus {
                <Self as ::daudio_sdk::DaudioEffect>::pre_block(self);

                for mut frame in buffer.iter_samples() {
                    if frame.len() < 2 {
                        continue;
                    }
                    let l = *frame.get_mut(0).unwrap();
                    let r = *frame.get_mut(1).unwrap();
                    let (ol, or) =
                        <Self as ::daudio_sdk::DaudioEffect>::process_frame(self, l, r);
                    *frame.get_mut(0).unwrap() = ol;
                    *frame.get_mut(1).unwrap() = or;
                }
                ::daudio_sdk::nih_plug::prelude::ProcessStatus::Normal
            }
        }

        impl ::daudio_sdk::nih_plug::prelude::ClapPlugin for #ident {
            const CLAP_ID: &'static str = #clap_id;
            const CLAP_DESCRIPTION: ::std::option::Option<&'static str> = #clap_description_ts;
            const CLAP_MANUAL_URL: ::std::option::Option<&'static str> =
                ::std::option::Option::Some(<Self as ::daudio_sdk::nih_plug::prelude::Plugin>::URL);
            const CLAP_SUPPORT_URL: ::std::option::Option<&'static str> =
                ::std::option::Option::None;
            const CLAP_FEATURES: &'static [::daudio_sdk::nih_plug::prelude::ClapFeature] =
                &[#(#clap_features_ts),*];
        }

        impl ::daudio_sdk::nih_plug::prelude::Vst3Plugin for #ident {
            const VST3_CLASS_ID: [u8; 16] = *#vst3_id_bytes;
            const VST3_SUBCATEGORIES: &'static [::daudio_sdk::nih_plug::prelude::Vst3SubCategory] =
                &[#(#vst3_categories_ts),*];
        }

        ::daudio_sdk::nih_plug::nih_export_clap!(#ident);
        ::daudio_sdk::nih_plug::nih_export_vst3!(#ident);
    };

    expanded.into()
}
