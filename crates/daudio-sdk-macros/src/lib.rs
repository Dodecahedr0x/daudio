use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{parse_macro_input, Ident, ItemStruct, LitBool, LitByteStr, LitStr, Token};

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
    /// When true, generate a SYNTH (MIDI) `Plugin` impl delegating to
    /// `DaudioSynth`; otherwise the effect impl delegating to `DaudioEffect`.
    midi: bool,
    /// When true, generate an audio→MIDI `Plugin` impl delegating to
    /// `DaudioAudioToMidi`. Mutually exclusive with `midi`.
    midi_out: bool,
}

/// A single `key = value` entry in the attribute list. `value` is either a
/// string literal, a boolean literal, or a bracketed list of idents.
enum ArgValue {
    Str(LitStr),
    Bool(LitBool),
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
        } else if input.peek(syn::LitBool) {
            ArgValue::Bool(input.parse()?)
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
        let mut midi = None;
        let mut midi_out = None;

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
                ("midi", ArgValue::Bool(b)) => assign!(midi, b.value()),
                ("midi_out", ArgValue::Bool(b)) => assign!(midi_out, b.value()),
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
            midi: midi.unwrap_or(false),
            midi_out: midi_out.unwrap_or(false),
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
        midi,
        midi_out,
    } = args;

    // `midi` (synth) and `midi_out` (audio→MIDI) are mutually exclusive.
    if midi && midi_out {
        return syn::Error::new(
            proc_macro2::Span::call_site(),
            "`daudio_plugin` cannot set both `midi` and `midi_out`",
        )
        .to_compile_error()
        .into();
    }

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

    // The `Plugin` impl differs between effects (stereo in/out, delegating to
    // `DaudioEffect`), synths (MIDI-in, no audio input, stereo out, delegating
    // to `DaudioSynth`), and audio→MIDI analyzers (stereo in/out passthrough +
    // MIDI out, delegating to `DaudioAudioToMidi`). Everything else
    // (Clap/Vst3/exports) is identical. Select the differing half here.
    let plugin_impl = if midi_out {
        quote! {
            impl ::daudio_sdk::nih_plug::prelude::Plugin for #ident {
                const NAME: &'static str = #name;
                const VENDOR: &'static str = #vendor;
                const URL: &'static str = #url_ts;
                const EMAIL: &'static str = #email_ts;
                const VERSION: &'static str = env!("CARGO_PKG_VERSION");

                // Mono input, stereo output. An analyzer sums to mono anyway, and
                // a mono main input matches the common mono microphone/source (and
                // the standalone backend, which requires the input device's channel
                // count to match) while stereo output matches typical outputs.
                const AUDIO_IO_LAYOUTS: &'static [::daudio_sdk::nih_plug::prelude::AudioIOLayout] =
                    &[::daudio_sdk::nih_plug::prelude::AudioIOLayout {
                        main_input_channels: ::std::num::NonZeroU32::new(1),
                        main_output_channels: ::std::num::NonZeroU32::new(2),
                        ..::daudio_sdk::nih_plug::prelude::AudioIOLayout::const_default()
                    }];

                const MIDI_OUTPUT: ::daudio_sdk::nih_plug::prelude::MidiConfig =
                    ::daudio_sdk::nih_plug::prelude::MidiConfig::MidiCCs;

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
                    <Self as ::daudio_sdk::DaudioAudioToMidi>::activate(self, buffer_config.sample_rate);
                    true
                }

                fn reset(&mut self) {
                    <Self as ::daudio_sdk::DaudioAudioToMidi>::reset(self);
                }

                fn editor(
                    &mut self,
                    _async_executor: ::daudio_sdk::nih_plug::prelude::AsyncExecutor<Self>,
                ) -> ::std::option::Option<
                    ::std::boxed::Box<dyn ::daudio_sdk::nih_plug::prelude::Editor>,
                > {
                    <Self as ::daudio_sdk::DaudioAudioToMidi>::editor(self)
                }

                fn process(
                    &mut self,
                    buffer: &mut ::daudio_sdk::nih_plug::prelude::Buffer,
                    _aux: &mut ::daudio_sdk::nih_plug::prelude::AuxiliaryBuffers,
                    context: &mut impl ::daudio_sdk::nih_plug::prelude::ProcessContext<Self>,
                ) -> ::daudio_sdk::nih_plug::prelude::ProcessStatus {
                    for (sample_id, mut channel_samples) in buffer.iter_samples().enumerate() {
                        let mut sum = 0.0f32;
                        let mut count = 0u32;
                        for s in channel_samples.iter_mut() {
                            sum += *s;
                            count += 1;
                        }
                        let mono = if count > 0 { sum / count as f32 } else { 0.0 };
                        <Self as ::daudio_sdk::DaudioAudioToMidi>::process_sample(
                            self,
                            mono,
                            sample_id as u32,
                            &mut |event| context.send_event(event),
                        );
                    }
                    ::daudio_sdk::nih_plug::prelude::ProcessStatus::Normal
                }
            }
        }
    } else if midi {
        quote! {
            impl ::daudio_sdk::nih_plug::prelude::Plugin for #ident {
                const NAME: &'static str = #name;
                const VENDOR: &'static str = #vendor;
                const URL: &'static str = #url_ts;
                const EMAIL: &'static str = #email_ts;
                const VERSION: &'static str = env!("CARGO_PKG_VERSION");

                const AUDIO_IO_LAYOUTS: &'static [::daudio_sdk::nih_plug::prelude::AudioIOLayout] =
                    &[::daudio_sdk::nih_plug::prelude::AudioIOLayout {
                        main_input_channels: ::std::option::Option::None,
                        main_output_channels: ::std::num::NonZeroU32::new(2),
                        ..::daudio_sdk::nih_plug::prelude::AudioIOLayout::const_default()
                    }];

                const MIDI_INPUT: ::daudio_sdk::nih_plug::prelude::MidiConfig =
                    ::daudio_sdk::nih_plug::prelude::MidiConfig::Basic;

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
                    <Self as ::daudio_sdk::DaudioSynth>::activate(self, buffer_config.sample_rate);
                    true
                }

                fn reset(&mut self) {
                    <Self as ::daudio_sdk::DaudioSynth>::reset(self);
                }

                fn editor(
                    &mut self,
                    _async_executor: ::daudio_sdk::nih_plug::prelude::AsyncExecutor<Self>,
                ) -> ::std::option::Option<
                    ::std::boxed::Box<dyn ::daudio_sdk::nih_plug::prelude::Editor>,
                > {
                    <Self as ::daudio_sdk::DaudioSynth>::editor(self)
                }

                fn process(
                    &mut self,
                    buffer: &mut ::daudio_sdk::nih_plug::prelude::Buffer,
                    _aux: &mut ::daudio_sdk::nih_plug::prelude::AuxiliaryBuffers,
                    context: &mut impl ::daudio_sdk::nih_plug::prelude::ProcessContext<Self>,
                ) -> ::daudio_sdk::nih_plug::prelude::ProcessStatus {
                    <Self as ::daudio_sdk::DaudioSynth>::pre_block(self);

                    let mut next_event = context.next_event();
                    for (sample_id, mut channel_samples) in buffer.iter_samples().enumerate() {
                        while let Some(event) = next_event {
                            if event.timing() > sample_id as u32 {
                                break;
                            }
                            match event {
                                ::daudio_sdk::nih_plug::prelude::NoteEvent::NoteOn {
                                    note, velocity, ..
                                } => <Self as ::daudio_sdk::DaudioSynth>::note_on(
                                    self, note, velocity,
                                ),
                                ::daudio_sdk::nih_plug::prelude::NoteEvent::NoteOff {
                                    note, ..
                                } => <Self as ::daudio_sdk::DaudioSynth>::note_off(self, note),
                                _ => {}
                            }
                            next_event = context.next_event();
                        }
                        let (l, r) = <Self as ::daudio_sdk::DaudioSynth>::render_frame(self);
                        if channel_samples.len() >= 2 {
                            *channel_samples.get_mut(0).unwrap() = l;
                            *channel_samples.get_mut(1).unwrap() = r;
                        }
                    }
                    ::daudio_sdk::nih_plug::prelude::ProcessStatus::Normal
                }
            }
        }
    } else {
        quote! {
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

                fn editor(
                    &mut self,
                    _async_executor: ::daudio_sdk::nih_plug::prelude::AsyncExecutor<Self>,
                ) -> ::std::option::Option<
                    ::std::boxed::Box<dyn ::daudio_sdk::nih_plug::prelude::Editor>,
                > {
                    <Self as ::daudio_sdk::DaudioEffect>::editor(self)
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
        }
    };

    let expanded = quote! {
        #item_struct

        #plugin_impl

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
